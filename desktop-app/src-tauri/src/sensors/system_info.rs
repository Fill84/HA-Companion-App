use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfoData {
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub motherboard_manufacturer: Option<String>,
    pub motherboard_model: Option<String>,
    pub bios_version: Option<String>,
}

pub fn collect() -> SystemInfoData {
    let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());
    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());

    let (motherboard_manufacturer, motherboard_model, bios_version) = collect_platform_info();

    SystemInfoData {
        os_name,
        os_version,
        hostname,
        motherboard_manufacturer,
        motherboard_model,
        bios_version,
    }
}

#[cfg(windows)]
fn collect_platform_info() -> (Option<String>, Option<String>, Option<String>) {
    use std::collections::HashMap;
    use wmi::{COMLibrary, WMIConnection, Variant};

    let com_lib = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return (None, None, None),
    };
    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(w) => w,
        Err(_) => return (None, None, None),
    };

    // Get motherboard info
    let mut mb_manufacturer = None;
    let mut mb_model = None;
    if let Ok(results) = wmi_con.raw_query::<HashMap<String, Variant>>(
        "SELECT Manufacturer, Product FROM Win32_BaseBoard",
    ) {
        if let Some(result) = results.first() {
            mb_manufacturer = match result.get("Manufacturer") {
                Some(Variant::String(s)) => Some(s.clone()),
                _ => None,
            };
            mb_model = match result.get("Product") {
                Some(Variant::String(s)) => Some(s.clone()),
                _ => None,
            };
        }
    }

    // Get BIOS version
    let mut bios_version = None;
    if let Ok(results) = wmi_con.raw_query::<HashMap<String, Variant>>(
        "SELECT SMBIOSBIOSVersion FROM Win32_BIOS",
    ) {
        if let Some(result) = results.first() {
            bios_version = match result.get("SMBIOSBIOSVersion") {
                Some(Variant::String(s)) => Some(s.clone()),
                _ => None,
            };
        }
    }

    (mb_manufacturer, mb_model, bios_version)
}

#[cfg(target_os = "linux")]
fn collect_platform_info() -> (Option<String>, Option<String>, Option<String>) {
    let mb_manufacturer = std::fs::read_to_string("/sys/class/dmi/id/board_vendor")
        .ok()
        .map(|s| s.trim().to_string());

    let mb_model = std::fs::read_to_string("/sys/class/dmi/id/board_name")
        .ok()
        .map(|s| s.trim().to_string());

    let bios_version = std::fs::read_to_string("/sys/class/dmi/id/bios_version")
        .ok()
        .map(|s| s.trim().to_string());

    (mb_manufacturer, mb_model, bios_version)
}

#[cfg(target_os = "macos")]
fn collect_platform_info() -> (Option<String>, Option<String>, Option<String>) {
    let output = std::process::Command::new("system_profiler")
        .arg("SPHardwareDataType")
        .arg("-json")
        .output()
        .ok();

    if let Some(output) = output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(items) = json.get("SPHardwareDataType").and_then(|v| v.as_array()) {
                    if let Some(item) = items.first() {
                        let model = item
                            .get("machine_model")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let boot_rom = item
                            .get("boot_rom_version")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        return (Some("Apple".to_string()), model, boot_rom);
                    }
                }
            }
        }
    }

    (Some("Apple".to_string()), None, None)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn collect_platform_info() -> (Option<String>, Option<String>, Option<String>) {
    (None, None, None)
}
