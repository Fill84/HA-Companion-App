//! Native Windows MSR reader via the WinRing0 kernel driver.
//!
//! This is what every Windows hardware-monitoring tool (LibreHardwareMonitor,
//! CoreTemp, HWiNFO, AIDA64, MSI Afterburner) does internally. We bundle the
//! driver inside `ha-companion.exe`, extract it to disk on first run, register
//! it as a Windows kernel-mode service, and talk to it via `DeviceIoControl`
//! to execute `RDMSR` instructions (which user-space cannot run directly —
//! they require Ring 0 / kernel mode).
//!
//! Driver source: extracted from the official `LibreHardwareMonitorLib` NuGet
//! package (MPL-2.0). Same binary every other monitoring tool uses.

#![cfg(windows)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_EXISTS, GENERIC_READ,
    GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Services::{
    CloseServiceHandle, CreateServiceW, OpenSCManagerW, OpenServiceW, StartServiceW,
    SC_MANAGER_ALL_ACCESS, SERVICE_ALL_ACCESS, SERVICE_DEMAND_START, SERVICE_ERROR_NORMAL,
    SERVICE_KERNEL_DRIVER,
};

/// WinRing0's IOCTL for reading an MSR.
/// Function 0x821, METHOD_BUFFERED, FILE_READ_ACCESS.
const IOCTL_OLS_READ_MSR: u32 = 0x9C402084;

/// Embedded WinRing0 64-bit kernel driver, extracted from the official
/// LibreHardwareMonitorLib 0.9.4 NuGet package at build time.
const DRIVER_BYTES: &[u8] = include_bytes!("../../drivers/WinRing0x64.sys");

/// Device paths to try in order. WinRing0 has been distributed by
/// LibreHardwareMonitor / OpenHardwareMonitor / HWiNFO / CoreTemp /
/// MSI Afterburner / Ryzen Master under multiple naming conventions
/// over the years — we accept any of them so we benefit from whatever
/// hwmon tool was already installed on this machine.
const DEVICE_PATHS: &[&str] = &[
    r"\\.\WinRing0x64",     // OHM / LHM legacy + older LHM releases
    r"\\.\WinRing0_1_2_0",  // Newer LHM convention
    r"\\.\WinRing0",        // Some custom builds
];
/// Service name we use when we have to install ourselves. Must match
/// the path the embedded driver expects to be referenced as.
const SERVICE_NAME: &str = "WinRing0_1_2_0";

/// Owns a handle to the WinRing0 device and exposes `read_msr`.
pub struct WinRing0 {
    device: HANDLE,
}

impl WinRing0 {
    /// Open the WinRing0 device. Tries cheapest path first:
    ///
    /// 1. Just open `\\.\WinRing0_1_2_0` — works if the driver service is
    ///    already installed and running on this machine (which it usually
    ///    is, since LibreHardwareMonitor / HWiNFO / CoreTemp / MSI Afterburner
    ///    / Ryzen Master / countless other tools install the same driver).
    ///    NO admin needed for this path. This is the common case.
    /// 2. If step 1 fails, try starting an already-installed service —
    ///    works without admin if the user has the service set to manual
    ///    start. Still might fail.
    /// 3. If steps 1+2 fail, extract our embedded driver and install the
    ///    service. ONLY this step needs admin elevation. We only ever take
    ///    this path on a fresh machine that has never had a hwmon tool.
    pub fn open() -> io::Result<Self> {
        // Fast path: device is already up, just grab the handle.
        if let Ok(device) = open_device() {
            return Ok(Self { device });
        }

        // Service may exist but be stopped — try starting (won't install).
        let _ = ensure_service_started();
        if let Ok(device) = open_device() {
            return Ok(Self { device });
        }

        // Fresh machine: extract driver and install the service. Needs admin.
        let driver_path = extract_driver()?;
        ensure_service_installed(&driver_path)?;
        ensure_service_started()?;
        let device = open_device()?;
        Ok(Self { device })
    }

    /// Execute `RDMSR <msr_index>` on the CPU and return the 64-bit result.
    /// Returns Err if the driver rejects the IOCTL (some MSRs are not
    /// readable on all CPUs).
    pub fn read_msr(&self, msr_index: u32) -> io::Result<u64> {
        let mut input = msr_index;
        let mut output = [0u8; 8];
        let mut bytes_returned: u32 = 0;
        let ok = unsafe {
            DeviceIoControl(
                self.device,
                IOCTL_OLS_READ_MSR,
                Some(&mut input as *mut u32 as *mut _),
                std::mem::size_of::<u32>() as u32,
                Some(output.as_mut_ptr() as *mut _),
                output.len() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };
        if ok.is_err() {
            return Err(io::Error::last_os_error());
        }
        // WinRing0 returns the MSR as two u32s: EAX (low) then EDX (high)
        let low = u32::from_le_bytes(output[0..4].try_into().unwrap()) as u64;
        let high = u32::from_le_bytes(output[4..8].try_into().unwrap()) as u64;
        Ok((high << 32) | low)
    }
}

impl Drop for WinRing0 {
    fn drop(&mut self) {
        if !self.device.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.device);
            }
        }
    }
}

// SAFETY: HANDLE is a Windows kernel object that the driver synchronises
// internally; concurrent DeviceIoControl is safe per WinRing0's design.
unsafe impl Send for WinRing0 {}
unsafe impl Sync for WinRing0 {}

// --- helpers -----------------------------------------------------------

fn extract_driver() -> io::Result<PathBuf> {
    let appdata = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no APPDATA"))?;
    let dir = Path::new(&appdata).join("com.ha-companion.desktop");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("WinRing0x64.sys");
    // Only rewrite if missing or different size — drivers don't change without
    // an app upgrade. Avoid touching a file SCM has locked.
    let needs_write = match std::fs::metadata(&path) {
        Ok(m) => m.len() as usize != DRIVER_BYTES.len(),
        Err(_) => true,
    };
    if needs_write {
        std::fs::write(&path, DRIVER_BYTES)?;
    }
    Ok(path)
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn ensure_service_installed(driver_path: &Path) -> io::Result<()> {
    let scm = unsafe {
        OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ALL_ACCESS)
            .map_err(|_| io::Error::last_os_error())?
    };
    let name = wide(SERVICE_NAME);
    let path_str = driver_path.to_string_lossy().into_owned();
    let path = wide(&path_str);

    let svc_result = unsafe {
        CreateServiceW(
            scm,
            PCWSTR(name.as_ptr()),
            PCWSTR(name.as_ptr()),
            SERVICE_ALL_ACCESS,
            SERVICE_KERNEL_DRIVER,
            SERVICE_DEMAND_START,
            SERVICE_ERROR_NORMAL,
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
    };

    match svc_result {
        Ok(svc) => {
            unsafe {
                let _ = CloseServiceHandle(svc);
                let _ = CloseServiceHandle(scm);
            }
            Ok(())
        }
        Err(_) => {
            let err = unsafe { GetLastError() };
            unsafe {
                let _ = CloseServiceHandle(scm);
            }
            // ERROR_SERVICE_EXISTS (1073) means a previous install already
            // registered it — that's fine, we proceed to start.
            if err.0 == ERROR_SERVICE_EXISTS.0 {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(err.0 as i32))
            }
        }
    }
}

fn ensure_service_started() -> io::Result<()> {
    let scm = unsafe {
        OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ALL_ACCESS)
            .map_err(|_| io::Error::last_os_error())?
    };
    let name = wide(SERVICE_NAME);
    let svc = unsafe {
        OpenServiceW(scm, PCWSTR(name.as_ptr()), SERVICE_ALL_ACCESS)
            .map_err(|e| {
                let _ = CloseServiceHandle(scm);
                io::Error::other(format!("OpenService failed: {}", e))
            })?
    };
    let started = unsafe { StartServiceW(svc, None) };
    unsafe {
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
    }
    if started.is_ok() {
        return Ok(());
    }
    let err = unsafe { GetLastError() };
    // ERROR_SERVICE_ALREADY_RUNNING (1056) means we're good.
    if err.0 == ERROR_SERVICE_ALREADY_RUNNING.0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(err.0 as i32))
    }
}

fn open_device() -> io::Result<HANDLE> {
    let mut last_err = io::Error::new(io::ErrorKind::NotFound, "no device paths tried");
    for path_str in DEVICE_PATHS {
        let path = wide(path_str);
        let result = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };
        match result {
            Ok(handle) if handle != INVALID_HANDLE_VALUE => {
                log::info!("[WinRing0] opened device {}", path_str);
                return Ok(handle);
            }
            _ => {
                last_err = io::Error::last_os_error();
            }
        }
    }
    Err(last_err)
}

// --- Intel CPU temperature ---------------------------------------------

/// Intel: MSR 0x19C IA32_THERM_STATUS — bits 22:16 = digital readout
/// (degrees Celsius below TjMax).
const IA32_THERM_STATUS: u32 = 0x19C;
/// Intel: MSR 0x1A2 MSR_TEMPERATURE_TARGET — bits 23:16 = TjMax (Tcc activation).
const IA32_TEMPERATURE_TARGET: u32 = 0x1A2;

/// Read the package CPU temperature on an Intel CPU.
///
/// Returns Err if the MSRs are not readable (non-Intel CPU, very old Intel,
/// virtualized environment) or if the driver-call fails.
pub fn read_intel_package_temp(ring0: &WinRing0) -> io::Result<f32> {
    let target = ring0.read_msr(IA32_TEMPERATURE_TARGET)?;
    let tj_max = ((target >> 16) & 0xFF) as i32;
    if !(60..=120).contains(&tj_max) {
        return Err(io::Error::other(format!(
            "implausible TjMax {tj_max}°C — non-Intel CPU?"
        )));
    }
    let therm = ring0.read_msr(IA32_THERM_STATUS)?;
    // Bit 31 = ReadValid; if not set the readout is undefined.
    if (therm >> 31) & 1 != 1 {
        return Err(io::Error::other("IA32_THERM_STATUS readout invalid"));
    }
    let digital_readout = ((therm >> 16) & 0x7F) as i32;
    let temp = (tj_max - digital_readout) as f32;
    Ok(temp)
}
