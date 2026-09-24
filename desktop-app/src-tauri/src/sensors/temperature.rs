use std::collections::HashMap;

#[derive(Debug)]
pub struct Temperature {
    pub value: Option<f32>,
    pub status: String,
    pub source: Option<String>,
    pub labels: Vec<String>,
}

impl Temperature {
    pub fn unavailable(status: &str) -> Self {
        Self {
            value: None,
            status: status.into(),
            source: None,
            labels: Vec::new(),
        }
    }

    pub fn attributes(&self) -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("provider_status".into(), serde_json::json!(self.status)),
            ("measurement_source".into(), serde_json::json!(self.source)),
            ("sensor_labels".into(), serde_json::json!(self.labels)),
            (
                "aggregation".into(),
                serde_json::json!("maximum_package_or_die"),
            ),
        ])
    }
}

pub struct TemperatureReader {
    #[cfg(not(windows))]
    components: sysinfo::Components,
    #[cfg(not(windows))]
    discovery_after: Option<std::time::Instant>,
}

impl TemperatureReader {
    pub fn new() -> Self {
        Self {
            #[cfg(not(windows))]
            components: sysinfo::Components::new(),
            #[cfg(not(windows))]
            discovery_after: None,
        }
    }

    pub fn suspend(&mut self) {}

    #[cfg(windows)]
    pub fn read(&mut self) -> Temperature {
        windows_service::read()
    }

    #[cfg(not(windows))]
    pub fn read(&mut self) -> Temperature {
        let now = std::time::Instant::now();
        if self.discovery_after.is_none_or(|when| now >= when) {
            self.components.refresh_list();
            self.discovery_after = Some(now + std::time::Duration::from_secs(600));
        } else {
            self.components.refresh();
        }
        let mut value: Option<f32> = None;
        let mut labels = Vec::new();
        for component in &self.components {
            let label = component.label();
            if !(label.starts_with("coretemp Package id ")
                || label == "k10temp Tdie"
                || label == "CPU Die"
                || label == "PECI CPU")
            {
                continue;
            }
            let current = component.temperature();
            if current.is_finite() && current > 0.0 && current < 150.0 {
                value = Some(value.map_or(current, |old| old.max(current)));
                labels.push(label.to_string());
            }
        }
        Temperature {
            value,
            status: if value.is_some() { "ok" } else { "unsupported" }.into(),
            source: Some("native/sysinfo".into()),
            labels,
        }
    }
}

#[cfg(windows)]
mod windows_service {
    use super::Temperature;
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::io::RawHandle;
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::NamedPipeClient;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            disposition: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
    }

    const PIPE: &str = r"\\.\pipe\HaCompanionSensorsV1";
    const REQUEST: &[u8; 4] = b"HCS1";

    pub(super) fn read() -> Temperature {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return Temperature::unavailable("provider_error"),
        };
        match runtime.block_on(tokio::time::timeout(Duration::from_secs(4), request())) {
            Ok(Ok(bytes)) => parse_response(&bytes),
            Ok(Err(_)) => Temperature::unavailable("provider_unavailable"),
            Err(_) => Temperature::unavailable("provider_timeout"),
        }
    }

    async fn request() -> std::io::Result<[u8; 16]> {
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut client = loop {
            match connect() {
                Ok(client) => break client,
                Err(error) if error.raw_os_error() == Some(231) && Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(error) => return Err(error),
            }
        };
        client.write_all(REQUEST).await?;
        let mut bytes = [0u8; 16];
        client.read_exact(&mut bytes).await?;
        Ok(bytes)
    }

    fn connect() -> io::Result<NamedPipeClient> {
        let path: Vec<u16> = PIPE.encode_utf16().chain(Some(0)).collect();
        // Tokio's default GENERIC_WRITE requests FILE_CREATE_PIPE_INSTANCE.
        // This client needs only read/write data, never server creation.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0x0010_0003, // SYNCHRONIZE | FILE_READ_DATA | FILE_WRITE_DATA
                0,
                std::ptr::null(),
                3,           // OPEN_EXISTING
                0x4011_0000, // OVERLAPPED | SQOS_PRESENT | IDENTIFICATION
                std::ptr::null_mut(),
            )
        };
        if handle == (-1isize as *mut c_void) {
            return Err(io::Error::last_os_error());
        }
        unsafe { NamedPipeClient::from_raw_handle(handle as RawHandle) }
    }

    fn parse_response(bytes: &[u8; 16]) -> Temperature {
        let version = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let status = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let value = f32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if version != 1 || bytes[12..] != [0; 4] {
            return Temperature::unavailable("incompatible_provider");
        }
        match status {
            0 if value.is_finite() && value > 0.0 && value < 150.0 => Temperature {
                value: Some(value),
                status: "ok".into(),
                source: Some("pawnio/2.2/intel-msr".into()),
                labels: vec!["CPU Package".into()],
            },
            1 => Temperature::unavailable("driver_unavailable"),
            2 => Temperature::unavailable("unsupported"),
            3 => Temperature::unavailable("provider_error"),
            _ => Temperature::unavailable("invalid_response"),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn rejects_invalid_service_data_and_accepts_package_temperature() {
            let mut bytes = [0u8; 16];
            bytes[..4].copy_from_slice(&1u32.to_le_bytes());
            bytes[8..12].copy_from_slice(&54.0f32.to_le_bytes());
            assert_eq!(parse_response(&bytes).value, Some(54.0));
            bytes[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
            assert_eq!(parse_response(&bytes).value, None);
            bytes[..4].copy_from_slice(&2u32.to_le_bytes());
            assert_eq!(parse_response(&bytes).status, "incompatible_provider");
        }
    }
}
