//! Installer-only SetupAPI calls. The running service never creates devices.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_service::service::{ServiceAccess, ServiceState, ServiceType};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList, SetupDiCreateDeviceInfoW,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceRegistryPropertyW, SetupDiSetDeviceRegistryPropertyW,
    UpdateDriverForPlugAndPlayDevicesW, DICD_GENERATE_ID, DIF_REGISTERDEVICE, DIF_REMOVE,
    SPDRP_HARDWAREID, SP_DEVINFO_DATA,
};

const CLASS: GUID = GUID {
    data1: 0x62f9c741,
    data2: 0xb25a,
    data3: 0x46ce,
    data4: [0xb5, 0x4c, 0x9b, 0xcc, 0xce, 0x08, 0xb6, 0xf2],
};

const HARDWARE_ID: &str = "Root\\PawnIO";

#[cfg(target_arch = "x86_64")]
const ARCH: &str = "x64";
#[cfg(target_arch = "aarch64")]
const ARCH: &str = "arm64";

#[cfg(target_arch = "x86_64")]
const HASHES: [&str; 3] = [
    "7c1c203e13693531243fbee3cb87d7b79170eae89f5729b3f41387fe68a54f0b",
    "a37d46840280efec92063d3a21014c803939e599b4c0da4a4d063b79eeca9446",
    "fca6e7d58b0cf38dbb913a2b9e532f48629145d395f454b16a9f58e97b8d3940",
];
#[cfg(target_arch = "aarch64")]
const HASHES: [&str; 3] = [
    "9989a2d6985f1131dc7618cb0a4e76f5e59c4837c5057ed765b80bc979ac00cb",
    "d95848578d62d33d9eed30b7478d323611818ff6a38c43363aca58777e2f8d35",
    "8113d5850e4d7d2cbf7573b12a1de57f254f51b925b017e105fb558ce3a16600",
];

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn last_error(action: &str) -> anyhow::Error {
    anyhow::anyhow!("{action}: {}", io::Error::last_os_error())
}

struct DeviceSet(windows_sys::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO);

impl DeviceSet {
    fn checked(
        raw: windows_sys::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    ) -> Result<Self> {
        if raw == (-1isize as _) {
            bail!(
                "SetupAPI could not create a device set: {}",
                io::Error::last_os_error()
            );
        }
        Ok(Self(raw))
    }
}

impl Drop for DeviceSet {
    fn drop(&mut self) {
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

fn info() -> SP_DEVINFO_DATA {
    let mut value: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
    value.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
    value
}

fn matching_device() -> Result<Option<(DeviceSet, SP_DEVINFO_DATA)>> {
    let set = DeviceSet::checked(unsafe {
        SetupDiGetClassDevsW(&CLASS, std::ptr::null(), std::ptr::null_mut(), 0)
    })?;
    let mut index = 0;
    loop {
        let mut device = info();
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut device) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(259) {
                return Ok(None);
            }
            return Err(error).context("enumerating PawnIO setup class");
        }
        index += 1;
        let mut data = [0u8; 4096];
        let ok = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set.0,
                &device,
                SPDRP_HARDWAREID,
                std::ptr::null_mut(),
                data.as_mut_ptr(),
                data.len() as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let error = io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(13 | 1168)) {
                continue;
            }
            return Err(error).context("reading installed device hardware IDs");
        }
        let ids: Vec<u16> = data
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        if String::from_utf16_lossy(&ids)
            .split('\0')
            .any(|id| id.eq_ignore_ascii_case(HARDWARE_ID))
        {
            return Ok(Some((set, device)));
        }
    }
}

fn package_dir() -> Result<PathBuf> {
    Ok(std::env::current_exe()?
        .parent()
        .context("sensor service directory missing")?
        .parent()
        .context("application resources directory missing")?
        .join("pawnio")
        .join(ARCH))
}

fn verify_package(dir: &Path) -> Result<PathBuf> {
    let names = ["PawnIO.inf", "pawnio.cat", "PawnIO.sys"];
    for (name, expected) in names.iter().zip(HASHES) {
        let bytes = std::fs::read(dir.join(name)).with_context(|| format!("reading {name}"))?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected {
            bail!("bundled PawnIO {name} does not match the pinned signed package");
        }
    }
    Ok(dir.join("PawnIO.inf"))
}

fn service_binary_path(path: &Path) -> Result<PathBuf> {
    let raw = path.to_string_lossy();
    let prefix = r"\SystemRoot\";
    if raw
        .to_ascii_lowercase()
        .starts_with(&prefix.to_ascii_lowercase())
    {
        let windows = std::env::var_os("SystemRoot").context("SystemRoot is missing")?;
        return Ok(PathBuf::from(windows).join(&raw[prefix.len()..]));
    }
    anyhow::ensure!(path.is_absolute(), "PawnIO service path is not absolute");
    Ok(path.to_path_buf())
}

fn verified_existing_service(service: &windows_service::service::Service) -> Result<()> {
    let config = service.query_config()?;
    anyhow::ensure!(
        config.service_type == ServiceType::KERNEL_DRIVER,
        "existing PawnIO service is not a kernel driver"
    );
    let binary = service_binary_path(&config.executable_path)?;
    let bytes = std::fs::read(&binary).context("reading existing PawnIO driver")?;
    anyhow::ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == HASHES[2],
        "existing PawnIO driver differs from the pinned signed version"
    );
    verify_package(binary.parent().context("PawnIO driver directory missing")?)?;
    Ok(())
}

fn reuse_existing_service(service: windows_service::service::Service) -> Result<()> {
    verified_existing_service(&service)?;
    if service.query_status()?.current_state == ServiceState::Stopped {
        service.start(&[] as &[&str])?;
    }
    let mut reader = ha_companion_sensor_core::pawnio::IntelReader::open()
        .context("opening existing PawnIO driver")?;
    anyhow::ensure!(
        reader.package_celsius()?.is_some(),
        "existing PawnIO driver did not return a valid Intel CPU temperature"
    );
    Ok(())
}

pub fn probe_existing() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        "PawnIO",
        ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS | ServiceAccess::START,
    )?;
    reuse_existing_service(service)
}

pub fn ensure() -> Result<bool> {
    if !cfg!(target_arch = "x86_64") {
        bail!("no validated Windows ARM64 CPU temperature reader is available");
    }
    let existing_device = matching_device()?.is_some();
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    match manager.open_service(
        "PawnIO",
        ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS | ServiceAccess::START,
    ) {
        Ok(service) => {
            verified_existing_service(&service)?;
            if existing_device {
                reuse_existing_service(service)?;
                return Ok(false);
            }
            // A staged driver service alone has no PnP device, so AddDevice has
            // never created the user-mode link. Register our root device below.
        }
        Err(windows_service::Error::Winapi(error)) if error.raw_os_error() == Some(1060) => {
            anyhow::ensure!(
                !existing_device,
                "PawnIO device exists without its driver service; refusing to replace it"
            );
        }
        Err(error) => return Err(error.into()),
    }
    let inf = verify_package(&package_dir()?)?;
    let set =
        DeviceSet::checked(unsafe { SetupDiCreateDeviceInfoList(&CLASS, std::ptr::null_mut()) })?;
    let name = wide(OsStr::new("PawnIO"));
    let hardware: Vec<u16> = HARDWARE_ID.encode_utf16().chain([0, 0]).collect();
    let mut device = info();
    if unsafe {
        SetupDiCreateDeviceInfoW(
            set.0,
            name.as_ptr(),
            &CLASS,
            std::ptr::null(),
            std::ptr::null_mut(),
            DICD_GENERATE_ID,
            &mut device,
        )
    } == 0
    {
        return Err(last_error("creating PawnIO root device"));
    }
    if unsafe {
        SetupDiSetDeviceRegistryPropertyW(
            set.0,
            &mut device,
            SPDRP_HARDWAREID,
            hardware.as_ptr().cast(),
            (hardware.len() * 2) as u32,
        )
    } == 0
    {
        return Err(last_error("setting PawnIO hardware ID"));
    }
    if unsafe { SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set.0, &device) } == 0 {
        return Err(last_error("registering PawnIO root device"));
    }
    let mut reboot = 0;
    let path = wide(inf.as_os_str());
    let id = wide(OsStr::new(HARDWARE_ID));
    if unsafe {
        UpdateDriverForPlugAndPlayDevicesW(
            std::ptr::null_mut(),
            id.as_ptr(),
            path.as_ptr(),
            0,
            &mut reboot,
        )
    } == 0
    {
        let error = last_error("installing signed PawnIO driver");
        unsafe { SetupDiCallClassInstaller(DIF_REMOVE, set.0, &device) };
        return Err(error);
    }
    if reboot != 0 {
        bail!("PawnIO installation requires a reboot");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_driver_bytes_match_installer_pins() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src-tauri/resources/pawnio")
            .join(ARCH);
        assert_eq!(verify_package(&dir).unwrap(), dir.join("PawnIO.inf"));
    }

    #[test]
    fn resolves_kernel_service_systemroot_path() {
        let resolved = service_binary_path(Path::new(
            r"\SystemRoot\System32\DriverStore\FileRepository\pawnio.inf_amd64_x\PawnIO.sys",
        ))
        .unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("PawnIO.sys"));
    }
}
