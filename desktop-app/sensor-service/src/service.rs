use std::ffi::{c_void, OsString};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ha_companion_sensor_core::pawnio::IntelReader;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use tokio::sync::Notify;
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

const SERVICE_NAME: &str = "HaCompanionSensors";
const PIPE_NAME: &str = r"\\.\pipe\HaCompanionSensorsV1";
const REQUEST: &[u8; 4] = b"HCS1";
const PROTOCOL: u32 = 1;
const STATUS_OK: u32 = 0;
const STATUS_UNAVAILABLE: u32 = 1;
const STATUS_UNSUPPORTED: u32 = 2;
const STATUS_ERROR: u32 = 3;
const SERVICE_PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;0x12019b;;;AU)";

#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit_handle: i32,
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
    fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
}

struct PipeSecurity(*mut c_void);

impl PipeSecurity {
    fn new(sddl: &str) -> io::Result<Self> {
        // Only SYSTEM and local authenticated users can access snapshots.
        // No client can submit a driver command through this pipe.
        let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        let success = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if success == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }

    fn attributes(&self) -> SecurityAttributes {
        SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor: self.0,
            inherit_handle: 0,
        }
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

define_windows_service!(ffi_service_main, service_main);

pub fn run() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

pub fn install(with_driver: bool) -> anyhow::Result<()> {
    if with_driver {
        crate::driver_install::ensure()?;
    }
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from("Home Assistant Companion Sensors"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe()?,
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let access = ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::CHANGE_CONFIG
        | ServiceAccess::QUERY_CONFIG
        | ServiceAccess::QUERY_STATUS;
    let service = match manager.open_service(SERVICE_NAME, access) {
        Ok(existing) => {
            let _ = existing.stop();
            for _ in 0..100 {
                if existing.query_status()?.current_state == ServiceState::Stopped {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            anyhow::ensure!(
                existing.query_status()?.current_state == ServiceState::Stopped,
                "sensor service did not stop before update"
            );
            existing.change_config(&info)?;
            existing
        }
        Err(error) if missing(&error) => manager.create_service(&info, access)?,
        Err(error) => return Err(error.into()),
    };
    service.set_description("Read-only CPU sensor snapshots for Home Assistant Companion")?;
    service.start(&[] as &[&str])?;
    let pipe_name: Vec<u16> = PIPE_NAME.encode_utf16().chain(Some(0)).collect();
    for _ in 0..50 {
        match service.query_status()?.current_state {
            ServiceState::Running => {
                if unsafe { WaitNamedPipeW(pipe_name.as_ptr(), 100) } != 0 {
                    return Ok(());
                }
            }
            ServiceState::Stopped => anyhow::bail!("sensor service stopped during startup"),
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    anyhow::bail!("sensor service did not become ready")
}

fn missing(error: &windows_service::Error) -> bool {
    matches!(error, windows_service::Error::Winapi(io) if io.raw_os_error() == Some(1060))
}

pub fn refresh_existing() -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(_) => install(false),
        Err(error) if missing(&error) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn uninstall() -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
    ) {
        Ok(service) => service,
        Err(error) if missing(&error) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let _ = service.stop();
    for _ in 0..100 {
        if service.query_status()?.current_state == ServiceState::Stopped {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::ensure!(
        service.query_status()?.current_state == ServiceState::Stopped,
        "sensor service did not stop before removal"
    );
    Ok(service.delete()?)
}

fn service_main(_arguments: Vec<OsString>) {
    let _ = run_service();
}

fn run_service() -> windows_service::Result<()> {
    let stopping = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(Notify::new());
    let signal = stopping.clone();
    let wake_signal = wake.clone();
    let handler = service_control_handler::register(SERVICE_NAME, move |event| match event {
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        ServiceControl::Stop => {
            signal.store(true, Ordering::Release);
            wake_signal.notify_one();
            ServiceControlHandlerResult::NoError
        }
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    handler.set_service_status(status(ServiceState::Running, 0))?;
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .and_then(|runtime| {
            let result = runtime.block_on(serve(stopping, wake, PIPE_NAME, SERVICE_PIPE_SDDL));
            runtime.shutdown_timeout(Duration::from_secs(1));
            result
        });
    handler.set_service_status(status(ServiceState::Stopped, u32::from(result.is_err())))?;
    Ok(())
}

fn status(state: ServiceState, exit_code: u32) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: ServiceExitCode::Win32(exit_code),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

async fn serve(
    stopping: Arc<AtomicBool>,
    wake: Arc<Notify>,
    pipe_name: &str,
    sddl: &str,
) -> io::Result<()> {
    let security = PipeSecurity::new(sddl)?;
    let mut reader: Option<IntelReader> = None;
    let mut worker = None;
    let mut pipe = create_pipe(&security, pipe_name, true)?;
    while !stopping.load(Ordering::Acquire) {
        tokio::select! {
            connected = pipe.connect() => connected?,
            _ = wake.notified() => break,
        }
        // Keep the pipe name owned while serving the connected client.
        let next_pipe = create_pipe(&security, pipe_name, false)?;
        let mut request = [0u8; 4];
        let received =
            tokio::time::timeout(Duration::from_secs(3), pipe.read_exact(&mut request)).await;
        if matches!(received, Ok(Ok(_))) && &request == REQUEST {
            let response = timed_snapshot(&mut reader, &mut worker).await;
            let _ = tokio::time::timeout(Duration::from_secs(3), pipe.write_all(&response)).await;
        }
        pipe = next_pipe;
    }
    Ok(())
}

async fn timed_snapshot(
    reader: &mut Option<IntelReader>,
    worker: &mut Option<tokio::task::JoinHandle<([u8; 16], Option<IntelReader>)>>,
) -> [u8; 16] {
    if let Some(previous) = worker.take() {
        if !previous.is_finished() {
            *worker = Some(previous);
            return response(STATUS_ERROR, 0.0);
        }
        *reader = previous.await.ok().and_then(|(_, reader)| reader);
    }
    let mut current_reader = reader.take();
    let mut task = tokio::task::spawn_blocking(move || {
        let result = snapshot(&mut current_reader);
        (result, current_reader)
    });
    match tokio::time::timeout(Duration::from_secs(3), &mut task).await {
        Ok(Ok((result, next_reader))) => {
            *reader = next_reader;
            result
        }
        Ok(Err(_)) => response(STATUS_ERROR, 0.0),
        Err(_) => {
            *worker = Some(task);
            response(STATUS_ERROR, 0.0)
        }
    }
}

fn create_pipe(security: &PipeSecurity, name: &str, first: bool) -> io::Result<NamedPipeServer> {
    let mut attributes = security.attributes();
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .max_instances(2)
        .pipe_mode(PipeMode::Message)
        .reject_remote_clients(true);
    unsafe {
        options.create_with_security_attributes_raw(
            name,
            (&mut attributes as *mut SecurityAttributes).cast(),
        )
    }
}

fn snapshot(reader: &mut Option<IntelReader>) -> [u8; 16] {
    if reader.is_none() {
        match IntelReader::open() {
            Ok(value) => *reader = Some(value),
            Err(error) => {
                let status = if error.kind() == io::ErrorKind::Unsupported {
                    STATUS_UNSUPPORTED
                } else {
                    STATUS_UNAVAILABLE
                };
                return response(status, 0.0);
            }
        }
    }
    match reader.as_mut().unwrap().package_celsius() {
        Ok(Some(celsius)) => response(STATUS_OK, celsius),
        Ok(None) => response(STATUS_UNSUPPORTED, 0.0),
        Err(_) => {
            *reader = None;
            response(STATUS_ERROR, 0.0)
        }
    }
}

fn response(status: u32, celsius: f32) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[..4].copy_from_slice(&PROTOCOL.to_le_bytes());
    bytes[4..8].copy_from_slice(&status.to_le_bytes());
    bytes[8..12].copy_from_slice(&celsius.to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::windows::named_pipe::ClientOptions;

    #[test]
    fn response_is_fixed_size_and_never_exposes_driver_commands() {
        assert_eq!(response(STATUS_OK, 55.5).len(), 16);
        assert_eq!(
            &response(STATUS_UNAVAILABLE, 0.0)[4..8],
            &1u32.to_le_bytes()
        );
    }

    #[tokio::test]
    async fn local_pipe_returns_a_bounded_read_only_snapshot() {
        tokio::task::LocalSet::new()
            .run_until(pipe_roundtrip())
            .await;
    }

    async fn pipe_roundtrip() {
        let name = format!(r"\\.\pipe\HaCompanionTest{}", std::process::id());
        let stopping = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Notify::new());
        let worker_stopping = stopping.clone();
        let worker_wake = wake.clone();
        let task_name = name.clone();
        let task = tokio::task::spawn_local(async move {
            serve(
                worker_stopping,
                worker_wake,
                &task_name,
                "D:P(A;;GA;;;SY)(A;;GA;;;AU)",
            )
            .await
        });
        for _ in 0..2 {
            let mut client = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    match ClientOptions::new().open(&name) {
                        Ok(client) => break client,
                        Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
                    }
                }
            })
            .await
            .unwrap();
            client.write_all(REQUEST).await.unwrap();
            let mut bytes = [0u8; 16];
            tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut bytes))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), PROTOCOL);
            assert!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) <= STATUS_ERROR);
        }
        stopping.store(true, Ordering::Release);
        wake.notify_one();
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
