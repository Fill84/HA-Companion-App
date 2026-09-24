use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuData {
    pub gpus: Vec<GpuInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub physical_id: Option<String>,
    pub name: String,
    pub vendor: String,
    pub usage_percent: Option<f32>,
    pub temperature: Option<f32>,
    pub vram_total_mb: Option<u64>,
    pub vram_used_mb: Option<u64>,
    pub driver_version: Option<String>,
}

pub fn collect() -> GpuData {
    let mut gpus = Vec::new();

    // Try NVIDIA via NVML
    if let Some(nvidia_gpus) = collect_nvidia() {
        gpus.extend(nvidia_gpus);
    }

    // Try WMI on Windows for AMD/Intel
    #[cfg(windows)]
    {
        if let Some(wmi_gpus) = collect_wmi() {
            // Only add WMI GPUs that weren't already found via NVML
            for wmi_gpu in wmi_gpus {
                let already_found = gpus.iter().any(|g: &GpuInfo| {
                    g.name.to_lowercase().contains(&wmi_gpu.name.to_lowercase())
                });
                if !already_found {
                    gpus.push(wmi_gpu);
                }
            }
        }
    }

    // Linux: try rocm-smi for AMD, sysfs for Intel
    #[cfg(target_os = "linux")]
    {
        if gpus.is_empty() {
            if let Some(linux_gpus) = collect_linux() {
                gpus.extend(linux_gpus);
            }
        }
    }

    // macOS: system_profiler
    #[cfg(target_os = "macos")]
    {
        if gpus.is_empty() {
            if let Some(mac_gpus) = collect_macos() {
                gpus.extend(mac_gpus);
            }
        }
    }

    GpuData { gpus }
}

fn collect_nvidia() -> Option<Vec<GpuInfo>> {
    let nvml = nvml_wrapper::Nvml::init().ok()?;
    let count = nvml.device_count().ok()?;
    let mut gpus = Vec::new();

    for i in 0..count {
        if let Ok(device) = nvml.device_by_index(i) {
            let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".to_string());
            let temperature = device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .ok()
                .map(|t| t as f32);
            let utilization = device.utilization_rates().ok().map(|u| u.gpu as f32);
            let memory = device.memory_info().ok();
            let vram_total = memory.as_ref().map(|m| m.total / 1_000_000);
            let vram_used = memory.as_ref().map(|m| m.used / 1_000_000);
            let driver_version = nvml.sys_driver_version().ok();

            gpus.push(GpuInfo {
                physical_id: device.uuid().ok().map(|id| format!("nvml:{id}")),
                name,
                vendor: "NVIDIA".to_string(),
                usage_percent: utilization,
                temperature,
                vram_total_mb: vram_total,
                vram_used_mb: vram_used,
                driver_version,
            });
        }
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(windows)]
fn collect_wmi() -> Option<Vec<GpuInfo>> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, WMIConnection};

    let com_lib = COMLibrary::new().ok()?;
    let wmi_con = WMIConnection::new(com_lib).ok()?;

    let results: Vec<HashMap<String, wmi::Variant>> = wmi_con
        .raw_query("SELECT Name, PNPDeviceID, AdapterRAM, DriverVersion FROM Win32_VideoController")
        .ok()?;

    let mut gpus = Vec::new();
    for result in results {
        let name = match result.get("Name") {
            Some(wmi::Variant::String(s)) => s.clone(),
            _ => "Unknown GPU".to_string(),
        };

        let vendor =
            if name.to_lowercase().contains("amd") || name.to_lowercase().contains("radeon") {
                "AMD".to_string()
            } else if name.to_lowercase().contains("intel") {
                "Intel".to_string()
            } else {
                "Unknown".to_string()
            };

        let vram_total = adapter_ram_mb(result.get("AdapterRAM"));

        let driver_version = match result.get("DriverVersion") {
            Some(wmi::Variant::String(s)) => Some(s.clone()),
            _ => None,
        };

        gpus.push(GpuInfo {
            physical_id: result.get("PNPDeviceID").and_then(|value| match value {
                wmi::Variant::String(id) if !id.is_empty() => Some(format!("pnp:{id}")),
                _ => None,
            }),
            name,
            vendor,
            usage_percent: None, // WMI doesn't provide real-time usage
            temperature: None,
            vram_total_mb: vram_total,
            vram_used_mb: None,
            driver_version,
        });
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(windows)]
fn adapter_ram_mb(value: Option<&wmi::Variant>) -> Option<u64> {
    let bytes = match value? {
        wmi::Variant::UI4(bytes) => *bytes as u64,
        wmi::Variant::I4(bytes) => u64::try_from(*bytes).ok()?,
        _ => return None,
    };
    Some(bytes / 1_000_000)
}

#[cfg(all(test, windows))]
mod wmi_tests {
    use super::adapter_ram_mb;

    #[test]
    fn negative_signed_adapter_ram_is_not_a_huge_vram_measurement() {
        assert_eq!(adapter_ram_mb(Some(&wmi::Variant::I4(-1))), None);
        assert_eq!(adapter_ram_mb(Some(&wmi::Variant::I4(1_000_000))), Some(1));
        assert_eq!(adapter_ram_mb(Some(&wmi::Variant::UI4(2_000_000))), Some(2));
    }
}

#[cfg(target_os = "linux")]
fn collect_linux() -> Option<Vec<GpuInfo>> {
    let mut gpus = Vec::new();

    // Try rocm-smi for AMD
    if let Ok(output) = std::process::Command::new("rocm-smi")
        .arg("--showtemp")
        .arg("--showuse")
        .arg("--showproductname")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Basic parsing of rocm-smi output
            if let Some(name) = stdout.lines().find(|l| l.contains("Card series")) {
                let gpu_name = name
                    .split(':')
                    .last()
                    .unwrap_or("AMD GPU")
                    .trim()
                    .to_string();
                gpus.push(GpuInfo {
                    physical_id: None,
                    name: gpu_name,
                    vendor: "AMD".to_string(),
                    usage_percent: None,
                    temperature: None,
                    vram_total_mb: None,
                    vram_used_mb: None,
                    driver_version: None,
                });
            }
        }
    }

    // Check sysfs for Intel GPU
    if std::path::Path::new("/sys/class/drm/card0/device/vendor").exists() {
        if let Ok(vendor) = std::fs::read_to_string("/sys/class/drm/card0/device/vendor") {
            if vendor.trim() == "0x8086" {
                // Intel vendor ID
                gpus.push(GpuInfo {
                    physical_id: std::fs::canonicalize("/sys/class/drm/card0/device")
                        .ok()
                        .map(|path| format!("sysfs:{}", path.display())),
                    name: "Intel Integrated Graphics".to_string(),
                    vendor: "Intel".to_string(),
                    usage_percent: None,
                    temperature: None,
                    vram_total_mb: None,
                    vram_used_mb: None,
                    driver_version: None,
                });
            }
        }
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(target_os = "macos")]
fn collect_macos() -> Option<Vec<GpuInfo>> {
    let output = std::process::Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .arg("-json")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let displays = json.get("SPDisplaysDataType")?.as_array()?;

    let mut gpus = Vec::new();
    for display in displays {
        let name = display
            .get("sppci_model")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown GPU")
            .to_string();

        let vendor = display
            .get("sppci_vendor")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let vram = display
            .get("sppci_vram")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                s.split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<u64>().ok())
            });

        gpus.push(GpuInfo {
            physical_id: None,
            name,
            vendor,
            usage_percent: None,
            temperature: None,
            vram_total_mb: vram.map(|v| v * 1000), // Decimal GB to MB
            vram_used_mb: None,
            driver_version: None,
        });
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}
