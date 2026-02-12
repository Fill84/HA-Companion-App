use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuData {
    pub model: String,
    pub usage_percent: f32,
    pub frequency_mhz: u64,
    pub temperature: Option<f32>,
    pub core_count: usize,
    pub logical_core_count: usize,
}

pub fn collect(sys: &System) -> CpuData {
    let cpus = sys.cpus();
    let model = cpus.first().map(|c| c.brand().to_string()).unwrap_or_default();
    let usage_percent = sys.global_cpu_usage();
    let frequency_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let core_count = sys.physical_core_count().unwrap_or(0);
    let logical_core_count = cpus.len();

    // Try to get CPU temperature from sysinfo components first
    let mut temperature = {
        let components = sysinfo::Components::new_with_refreshed_list();
        let all_labels: Vec<String> = components.iter().map(|c| c.label().to_string()).collect();
        if all_labels.is_empty() {
            log::debug!("[CPU] sysinfo: no thermal components found");
        } else {
            log::debug!("[CPU] sysinfo thermal components: {:?}", all_labels);
        }
        let found = components
            .iter()
            .find(|c| {
                let label = c.label().to_lowercase();
                label.contains("cpu") || label.contains("core") || label.contains("package")
            });
        if let Some(comp) = found {
            log::info!("[CPU] sysinfo temperature from '{}': {:.1}°C", comp.label(), comp.temperature());
            Some(comp.temperature())
        } else {
            log::debug!("[CPU] sysinfo: no CPU/core/package component found");
            None
        }
    };

    // Fallback: on Windows, try WMI thermal zone if sysinfo returned None
    #[cfg(windows)]
    if temperature.is_none() {
        temperature = collect_cpu_temp_wmi();
    }

    CpuData {
        model,
        usage_percent,
        frequency_mhz,
        temperature,
        core_count,
        logical_core_count,
    }
}

/// Try to read CPU temperature from WMI.
/// Attempts multiple WMI classes in order of reliability.
#[cfg(windows)]
fn collect_cpu_temp_wmi() -> Option<f32> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    // Attempt 1: MSAcpi_ThermalZoneTemperature (root\WMI, requires admin)
    // Values are in tenths of Kelvin.
    if let Ok(com_lib) = COMLibrary::new() {
        if let Ok(wmi_con) = WMIConnection::with_namespace_path("root\\WMI", com_lib) {
            match wmi_con.raw_query::<HashMap<String, Variant>>(
                "SELECT CurrentTemperature FROM MSAcpi_ThermalZoneTemperature",
            ) {
                Ok(results) => {
                    for result in &results {
                        if let Some(variant) = result.get("CurrentTemperature") {
                            let raw_temp = match variant {
                                Variant::UI4(n) => Some(*n as f32),
                                Variant::UI2(n) => Some(*n as f32),
                                Variant::I4(n) => Some(*n as f32),
                                _ => None,
                            };
                            if let Some(tenths_kelvin) = raw_temp {
                                let celsius = (tenths_kelvin / 10.0) - 273.15;
                                if celsius > 0.0 && celsius < 150.0 {
                                    log::info!("[CPU] Temperature from MSAcpi_ThermalZone: {:.1}°C", celsius);
                                    return Some(celsius);
                                }
                            }
                        }
                    }
                    log::debug!("[CPU] MSAcpi_ThermalZone returned {} results but no valid temp", results.len());
                }
                Err(e) => {
                    log::debug!("[CPU] MSAcpi_ThermalZoneTemperature query failed (needs admin?): {}", e);
                }
            }
        }
    }

    // Attempt 2: Win32_PerfFormattedData_Counters_ThermalZoneInformation (root\CIMV2, no admin needed)
    // Temperature is in Kelvin (not tenths). Available on Windows 10+.
    if let Ok(com_lib) = COMLibrary::new() {
        if let Ok(wmi_con) = WMIConnection::new(com_lib) {
            match wmi_con.raw_query::<HashMap<String, Variant>>(
                "SELECT Temperature FROM Win32_PerfFormattedData_Counters_ThermalZoneInformation",
            ) {
                Ok(results) => {
                    for result in &results {
                        if let Some(variant) = result.get("Temperature") {
                            let kelvin = match variant {
                                Variant::UI4(n) => Some(*n as f32),
                                Variant::UI2(n) => Some(*n as f32),
                                Variant::I4(n) => Some(*n as f32),
                                Variant::UI8(n) => Some(*n as f32),
                                _ => None,
                            };
                            if let Some(k) = kelvin {
                                let celsius = k - 273.15;
                                if celsius > 0.0 && celsius < 150.0 {
                                    log::info!("[CPU] Temperature from ThermalZoneInformation: {:.1}°C", celsius);
                                    return Some(celsius);
                                }
                            }
                        }
                    }
                    log::debug!("[CPU] ThermalZoneInformation returned {} results but no valid temp", results.len());
                }
                Err(e) => {
                    log::debug!("[CPU] ThermalZoneInformation query failed: {}", e);
                }
            }
        }
    }

    log::warn!("[CPU] No CPU temperature available from any WMI source");
    None
}
