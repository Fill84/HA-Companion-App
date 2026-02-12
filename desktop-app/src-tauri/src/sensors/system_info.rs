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
    pub bios_vendor: Option<String>,
    pub bios_release_date: Option<String>,
    pub uptime_seconds: u64,
    pub boot_time: u64,
    pub logged_in_user: Option<String>,
    pub process_count: usize,
    pub displays: Vec<DisplayInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub name: String,
    pub resolution: String,
    pub refresh_rate_hz: Option<u32>,
}

pub fn collect() -> SystemInfoData {
    let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());
    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
    let uptime_seconds = System::uptime();
    let boot_time = System::boot_time();

    let platform_info = collect_platform_info();
    let logged_in_user = collect_logged_in_user();
    let process_count = {
        let sys = System::new_with_specifics(
            sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::new()),
        );
        sys.processes().len()
    };
    let displays = collect_displays();

    SystemInfoData {
        os_name,
        os_version,
        hostname,
        motherboard_manufacturer: platform_info.motherboard_manufacturer,
        motherboard_model: platform_info.motherboard_model,
        bios_version: platform_info.bios_version,
        bios_vendor: platform_info.bios_vendor,
        bios_release_date: platform_info.bios_release_date,
        uptime_seconds,
        boot_time,
        logged_in_user,
        process_count,
        displays,
    }
}

/// Dynamic system info that changes over time
pub fn collect_dynamic() -> DynamicSystemInfo {
    let uptime_seconds = System::uptime();
    let process_count = {
        let sys = System::new_with_specifics(
            sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::new()),
        );
        sys.processes().len()
    };

    DynamicSystemInfo {
        uptime_seconds,
        process_count,
    }
}

#[derive(Debug, Clone)]
pub struct DynamicSystemInfo {
    pub uptime_seconds: u64,
    pub process_count: usize,
}

struct PlatformInfo {
    motherboard_manufacturer: Option<String>,
    motherboard_model: Option<String>,
    bios_version: Option<String>,
    bios_vendor: Option<String>,
    bios_release_date: Option<String>,
}

#[cfg(windows)]
fn variant_to_string(v: &wmi::Variant) -> Option<String> {
    use wmi::Variant;
    match v {
        Variant::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        }
        Variant::I1(n) => Some(n.to_string()),
        Variant::I2(n) => Some(n.to_string()),
        Variant::I4(n) => Some(n.to_string()),
        Variant::I8(n) => Some(n.to_string()),
        Variant::UI1(n) => Some(n.to_string()),
        Variant::UI2(n) => Some(n.to_string()),
        Variant::UI4(n) => Some(n.to_string()),
        Variant::UI8(n) => Some(n.to_string()),
        Variant::Bool(b) => Some(b.to_string()),
        Variant::R4(f) => Some(f.to_string()),
        Variant::R8(f) => Some(f.to_string()),
        _ => {
            log::warn!("[SystemInfo] Unhandled WMI variant type: {:?}", v);
            None
        }
    }
}

#[cfg(windows)]
fn collect_platform_info() -> PlatformInfo {
    use std::collections::HashMap;
    use wmi::{COMLibrary, WMIConnection, Variant};

    let com_lib = match COMLibrary::new() {
        Ok(c) => c,
        Err(e) => {
            log::error!("[SystemInfo] WMI COM init failed: {}", e);
            return PlatformInfo {
                motherboard_manufacturer: None,
                motherboard_model: None,
                bios_version: None,
                bios_vendor: None,
                bios_release_date: None,
            };
        }
    };
    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(w) => w,
        Err(e) => {
            log::error!("[SystemInfo] WMI connection failed: {}", e);
            return PlatformInfo {
                motherboard_manufacturer: None,
                motherboard_model: None,
                bios_version: None,
                bios_vendor: None,
                bios_release_date: None,
            };
        }
    };

    // Get motherboard info
    let mut mb_manufacturer = None;
    let mut mb_model = None;
    match wmi_con.raw_query::<HashMap<String, Variant>>(
        "SELECT Manufacturer, Product FROM Win32_BaseBoard",
    ) {
        Ok(results) => {
            if let Some(result) = results.first() {
                mb_manufacturer = result.get("Manufacturer").and_then(variant_to_string);
                mb_model = result.get("Product").and_then(variant_to_string);
                log::info!(
                    "[SystemInfo] Motherboard: manufacturer={:?}, model={:?}",
                    mb_manufacturer, mb_model
                );
            } else {
                log::warn!("[SystemInfo] Win32_BaseBoard query returned empty results");
            }
        }
        Err(e) => log::error!("[SystemInfo] Win32_BaseBoard query failed: {}", e),
    }

    // Get BIOS info (version, vendor, date)
    let mut bios_version = None;
    let mut bios_vendor = None;
    let mut bios_release_date = None;
    match wmi_con.raw_query::<HashMap<String, Variant>>(
        "SELECT SMBIOSBIOSVersion, Manufacturer, ReleaseDate FROM Win32_BIOS",
    ) {
        Ok(results) => {
            if let Some(result) = results.first() {
                bios_version = result.get("SMBIOSBIOSVersion").and_then(variant_to_string);
                bios_vendor = result.get("Manufacturer").and_then(variant_to_string);
                // ReleaseDate is in CIM_DATETIME format: "20210101000000.000000+000"
                if let Some(raw_date) = result.get("ReleaseDate").and_then(variant_to_string) {
                    if raw_date.len() >= 8 {
                        bios_release_date = Some(format!(
                            "{}-{}-{}",
                            &raw_date[..4],
                            &raw_date[4..6],
                            &raw_date[6..8]
                        ));
                    } else {
                        bios_release_date = Some(raw_date);
                    }
                }
                log::info!(
                    "[SystemInfo] BIOS: version={:?}, vendor={:?}, date={:?}",
                    bios_version, bios_vendor, bios_release_date
                );
            } else {
                log::warn!("[SystemInfo] Win32_BIOS query returned empty results");
            }
        }
        Err(e) => log::error!("[SystemInfo] Win32_BIOS query failed: {}", e),
    }

    PlatformInfo {
        motherboard_manufacturer: mb_manufacturer,
        motherboard_model: mb_model,
        bios_version,
        bios_vendor,
        bios_release_date,
    }
}

#[cfg(target_os = "linux")]
fn collect_platform_info() -> PlatformInfo {
    let mb_manufacturer = std::fs::read_to_string("/sys/class/dmi/id/board_vendor")
        .ok()
        .map(|s| s.trim().to_string());

    let mb_model = std::fs::read_to_string("/sys/class/dmi/id/board_name")
        .ok()
        .map(|s| s.trim().to_string());

    let bios_version = std::fs::read_to_string("/sys/class/dmi/id/bios_version")
        .ok()
        .map(|s| s.trim().to_string());

    let bios_vendor = std::fs::read_to_string("/sys/class/dmi/id/bios_vendor")
        .ok()
        .map(|s| s.trim().to_string());

    let bios_release_date = std::fs::read_to_string("/sys/class/dmi/id/bios_date")
        .ok()
        .map(|s| s.trim().to_string());

    PlatformInfo {
        motherboard_manufacturer: mb_manufacturer,
        motherboard_model: mb_model,
        bios_version,
        bios_vendor,
        bios_release_date,
    }
}

#[cfg(target_os = "macos")]
fn collect_platform_info() -> PlatformInfo {
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

                        return PlatformInfo {
                            motherboard_manufacturer: Some("Apple".to_string()),
                            motherboard_model: model,
                            bios_version: boot_rom,
                            bios_vendor: Some("Apple".to_string()),
                            bios_release_date: None,
                        };
                    }
                }
            }
        }
    }

    PlatformInfo {
        motherboard_manufacturer: Some("Apple".to_string()),
        motherboard_model: None,
        bios_version: None,
        bios_vendor: Some("Apple".to_string()),
        bios_release_date: None,
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn collect_platform_info() -> PlatformInfo {
    PlatformInfo {
        motherboard_manufacturer: None,
        motherboard_model: None,
        bios_version: None,
        bios_vendor: None,
        bios_release_date: None,
    }
}

// --- Logged-in user ---

#[cfg(windows)]
fn collect_logged_in_user() -> Option<String> {
    std::env::var("USERNAME").ok()
}

#[cfg(not(windows))]
fn collect_logged_in_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
}

// --- Display info ---

#[cfg(windows)]
fn collect_displays() -> Vec<DisplayInfo> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, WMIConnection, Variant};

    let com_lib = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(w) => w,
        Err(_) => return Vec::new(),
    };

    let mut displays = Vec::new();

    if let Ok(results) = wmi_con.raw_query::<HashMap<String, Variant>>(
        "SELECT Name, CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate FROM Win32_VideoController",
    ) {
        for (i, result) in results.iter().enumerate() {
            let name = result
                .get("Name")
                .and_then(variant_to_string)
                .unwrap_or_else(|| format!("Display {}", i + 1));

            let h_res = result.get("CurrentHorizontalResolution").and_then(|v| match v {
                Variant::UI4(n) => Some(*n),
                Variant::UI2(n) => Some(*n as u32),
                _ => variant_to_string(v).and_then(|s| s.parse().ok()),
            });
            let v_res = result.get("CurrentVerticalResolution").and_then(|v| match v {
                Variant::UI4(n) => Some(*n),
                Variant::UI2(n) => Some(*n as u32),
                _ => variant_to_string(v).and_then(|s| s.parse().ok()),
            });
            let refresh = result.get("CurrentRefreshRate").and_then(|v| match v {
                Variant::UI4(n) => Some(*n),
                Variant::UI2(n) => Some(*n as u32),
                _ => variant_to_string(v).and_then(|s| s.parse().ok()),
            });

            if let (Some(h), Some(v)) = (h_res, v_res) {
                displays.push(DisplayInfo {
                    name,
                    resolution: format!("{}x{}", h, v),
                    refresh_rate_hz: refresh,
                });
            }
        }
    }

    displays
}

#[cfg(not(windows))]
fn collect_displays() -> Vec<DisplayInfo> {
    // On Linux/macOS, display info would require platform-specific tools
    // (xrandr, system_profiler). For now, return empty.
    Vec::new()
}
