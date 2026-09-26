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

#[derive(Default)]
pub struct GpuCollector {
    #[cfg(windows)]
    wmi_gpus: Option<Vec<GpuInfo>>,
    #[cfg(target_os = "macos")]
    mac_gpus: Option<Vec<GpuInfo>>,
}

impl GpuCollector {
    pub fn collect(&mut self) -> GpuData {
        let mut gpus = Vec::new();

        // NVML supplies live values. WMI and system_profiler supply static
        // metadata and are queried once per discovered hardware inventory.
        if let Some(nvidia_gpus) = collect_nvidia() {
            gpus.extend(nvidia_gpus);
        }

        #[cfg(windows)]
        {
            if self.wmi_gpus.is_none() {
                self.wmi_gpus = collect_wmi();
            }
            if let Some(wmi_gpus) = &self.wmi_gpus {
                // Only add WMI GPUs that weren't already found via NVML
                for wmi_gpu in wmi_gpus {
                    let already_found = gpus.iter().any(|g: &GpuInfo| {
                        g.name.to_lowercase().contains(&wmi_gpu.name.to_lowercase())
                    });
                    if !already_found {
                        gpus.push(wmi_gpu.clone());
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(linux_gpus) = collect_linux(gpus.iter().any(|gpu| gpu.vendor == "NVIDIA")) {
                gpus.extend(linux_gpus);
            }
        }

        #[cfg(target_os = "macos")]
        {
            if gpus.is_empty() {
                if self.mac_gpus.is_none() {
                    self.mac_gpus = collect_macos();
                }
                if let Some(mac_gpus) = &self.mac_gpus {
                    gpus.extend(mac_gpus.iter().cloned());
                }
            }
        }

        GpuData { gpus }
    }

    pub fn invalidate_inventory(&mut self) {
        #[cfg(windows)]
        {
            self.wmi_gpus = None;
        }
        #[cfg(target_os = "macos")]
        {
            self.mac_gpus = None;
        }
    }
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
        let pnp_id = result.get("PNPDeviceID").and_then(|value| match value {
            wmi::Variant::String(id) if !id.is_empty() => Some(id.as_str()),
            _ => None,
        });
        if super::is_ephemeral_remote_display(pnp_id, &name) {
            continue;
        }

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
            physical_id: pnp_id.map(|id| format!("pnp:{id}")),
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
fn collect_linux(nvidia_available: bool) -> Option<Vec<GpuInfo>> {
    collect_linux_from(std::path::Path::new("/sys/class/drm"), nvidia_available)
}

#[cfg(target_os = "linux")]
fn collect_linux_from(root: &std::path::Path, nvidia_available: bool) -> Option<Vec<GpuInfo>> {
    let mut gpus = Vec::new();
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card")
            || name.len() == 4
            || !name[4..].chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let device = entry.path().join("device");
        let vendor = match std::fs::read_to_string(device.join("vendor"))
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("0x1002") => "AMD",
            Some("0x8086") => "Intel",
            Some("0x10de") => "NVIDIA",
            _ => continue,
        };
        if vendor == "NVIDIA" && nvidia_available {
            continue;
        }
        let pci_device = std::fs::read_to_string(device.join("device"))
            .ok()
            .map(|value| value.trim().to_string());
        let model = pci_device
            .as_deref()
            .map(|id| format!("{vendor} GPU {id}"))
            .unwrap_or_else(|| format!("{vendor} GPU"));
        let physical_id = std::fs::canonicalize(&device)
            .ok()
            .map(|path| format!("sysfs:{}", path.display()));
        let read_number = |file: &str| -> Option<u64> {
            std::fs::read_to_string(device.join(file))
                .ok()?
                .trim()
                .parse()
                .ok()
        };
        let temperature = if vendor == "AMD" {
            std::fs::read_dir(device.join("hwmon"))
                .ok()
                .and_then(|entries| {
                    entries.flatten().find_map(|hwmon| {
                        std::fs::read_to_string(hwmon.path().join("temp1_input"))
                            .ok()?
                            .trim()
                            .parse::<f32>()
                            .ok()
                    })
                })
                .map(|millidegrees| millidegrees / 1000.0)
                .filter(|celsius| celsius.is_finite() && *celsius > 0.0 && *celsius < 150.0)
        } else {
            None
        };
        gpus.push(GpuInfo {
            physical_id,
            name: model,
            vendor: vendor.to_string(),
            usage_percent: if vendor == "AMD" {
                read_number("gpu_busy_percent")
                    .filter(|percent| *percent <= 100)
                    .map(|percent| percent as f32)
            } else {
                None
            },
            temperature,
            vram_total_mb: read_number("mem_info_vram_total").map(|bytes| bytes / 1_000_000),
            vram_used_mb: read_number("mem_info_vram_used").map(|bytes| bytes / 1_000_000),
            driver_version: None,
        });
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::collect_linux_from;
    use std::fs;

    #[test]
    fn drm_snapshot_reads_amd_units_and_keeps_a_second_adapter() {
        let root = std::env::temp_dir().join(format!(
            "ha-companion-gpu-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let amd = root.join("card0/device");
        let intel = root.join("card1/device");
        fs::create_dir_all(amd.join("hwmon/hwmon0")).unwrap();
        fs::create_dir_all(&intel).unwrap();
        fs::write(amd.join("vendor"), "0x1002\n").unwrap();
        fs::write(amd.join("device"), "0x744c\n").unwrap();
        fs::write(amd.join("gpu_busy_percent"), "42\n").unwrap();
        fs::write(amd.join("mem_info_vram_total"), "16000000000\n").unwrap();
        fs::write(amd.join("hwmon/hwmon0/temp1_input"), "54000\n").unwrap();
        fs::write(intel.join("vendor"), "0x8086\n").unwrap();
        let gpus = collect_linux_from(&root, false).unwrap();
        assert_eq!(gpus.len(), 2);
        let amd_gpu = gpus.iter().find(|gpu| gpu.vendor == "AMD").unwrap();
        assert_eq!(amd_gpu.temperature, Some(54.0));
        assert_eq!(amd_gpu.usage_percent, Some(42.0));
        assert_eq!(amd_gpu.vram_total_mb, Some(16_000));
        assert!(gpus.iter().any(|gpu| gpu.vendor == "Intel"));
        fs::remove_dir_all(&root).unwrap();
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
            .and_then(parse_displayed_vram_mb);

        gpus.push(GpuInfo {
            physical_id: None,
            name,
            vendor,
            usage_percent: None,
            temperature: None,
            vram_total_mb: vram,
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

#[cfg(any(test, target_os = "macos"))]
fn parse_displayed_vram_mb(text: &str) -> Option<u64> {
    let mut fields = text.split_whitespace();
    let amount: f64 = fields.next()?.parse().ok()?;
    let multiplier = match fields.next()?.to_ascii_lowercase().as_str() {
        "mb" => 1.0,
        "gb" => 1000.0,
        "mib" => 1.048_576,
        "gib" => 1_073.741_824,
        _ => return None,
    };
    let result = amount * multiplier;
    if !result.is_finite() || result <= 0.0 || result > u64::MAX as f64 {
        return None;
    }
    Some(result.round() as u64)
}

#[cfg(test)]
mod vram_tests {
    use super::parse_displayed_vram_mb;

    #[test]
    fn respects_displayed_vram_unit() {
        assert_eq!(parse_displayed_vram_mb("8 GB"), Some(8000));
        assert_eq!(parse_displayed_vram_mb("1536 MB"), Some(1536));
        assert_eq!(parse_displayed_vram_mb("1.5 GiB"), Some(1611));
        assert_eq!(parse_displayed_vram_mb("unknown"), None);
        assert_eq!(parse_displayed_vram_mb("8 TB"), None);
    }
}
