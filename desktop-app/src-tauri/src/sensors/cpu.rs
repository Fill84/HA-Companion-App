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

pub fn collect(
    sys: &System,
    hwmon: Option<&crate::sensors::hwmon_client::HwmonSnapshot>,
) -> CpuData {
    let cpus = sys.cpus();
    let model = cpus.first().map(|c| c.brand().to_string()).unwrap_or_default();
    let usage_percent = sys.global_cpu_usage();
    let frequency_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let core_count = sys.physical_core_count().unwrap_or(0);
    let logical_core_count = cpus.len();

    // Phase 2A: prefer the bundled hwmon helper snapshot when present.
    // Falls through to the existing sysinfo/WMI chain when the helper
    // wasn't started, crashed, or returned both fields as null.
    let temperature: Option<f32> = hwmon.and_then(|s| {
        let t = s.best_cpu_temp();
        if let Some(v) = t {
            log::info!("[CPU] Temperature from hwmon-helper: {:.1}°C", v);
        }
        t
    });

    let temperature = temperature.or_else(|| {
        let components = sysinfo::Components::new_with_refreshed_list();
        let all_labels: Vec<String> = components.iter().map(|c| c.label().to_string()).collect();
        if all_labels.is_empty() {
            log::debug!("[CPU] sysinfo: no thermal components found");
        } else {
            log::debug!("[CPU] sysinfo thermal components: {:?}", all_labels);
        }
        components
            .iter()
            .find(|c| {
                let label = c.label().to_lowercase();
                label.contains("cpu") || label.contains("core") || label.contains("package")
            })
            .map(|comp| {
                log::info!("[CPU] sysinfo temperature from '{}': {:.1}°C", comp.label(), comp.temperature());
                comp.temperature()
            })
    });

    #[cfg(windows)]
    let temperature = temperature.or_else(|| {
        collect_cpu_temp_lhm()
            .or_else(collect_cpu_temp_ohm)
            .or_else(collect_cpu_temp_wmi)
    });

    match temperature {
        Some(t) => log::info!("[CPU] Final temperature = {:.1}°C", t),
        None => log::warn!("[CPU] Final temperature = None (will be reported as unknown to HA)"),
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

/// Try LibreHardwareMonitor's WMI namespace (root\LibreHardwareMonitor).
/// LHM must be running with its WMI provider enabled in Options → WMI.
/// Doesn't need admin on our side because LHM does the privileged reads.
#[cfg(windows)]
fn collect_cpu_temp_lhm() -> Option<f32> {
    collect_cpu_temp_hwmon("root\\LibreHardwareMonitor", "LibreHardwareMonitor")
}

/// Try OpenHardwareMonitor's WMI namespace (root\OpenHardwareMonitor).
/// Same idea as LHM — works without admin if OHM is running.
#[cfg(windows)]
fn collect_cpu_temp_ohm() -> Option<f32> {
    collect_cpu_temp_hwmon("root\\OpenHardwareMonitor", "OpenHardwareMonitor")
}

/// Shared query for LHM/OHM. Both expose a `Sensor` class with the same
/// field names (Identifier, SensorType, Value, Name). We pick the most
/// representative CPU temperature — prefer Package/Tdie/CCD aggregates,
/// fall back to averaging core temperatures.
#[cfg(windows)]
fn collect_cpu_temp_hwmon(namespace: &str, source_name: &str) -> Option<f32> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com_lib = match COMLibrary::new() {
        Ok(c) => c,
        Err(e) => {
            log::info!("[CPU] {}: COM init failed: {}", source_name, e);
            return None;
        }
    };
    let wmi_con = match WMIConnection::with_namespace_path(namespace, com_lib) {
        Ok(w) => w,
        Err(e) => {
            log::info!(
                "[CPU] {}: namespace {} not available ({}). Hardware monitor probably not running.",
                source_name, namespace, e
            );
            return None;
        }
    };

    let results: Vec<HashMap<String, Variant>> = match wmi_con.raw_query(
        "SELECT Identifier, SensorType, Value, Name FROM Sensor WHERE SensorType = 'Temperature'",
    ) {
        Ok(r) => r,
        Err(e) => {
            log::info!("[CPU] {}: Sensor query failed: {}", source_name, e);
            return None;
        }
    };

    log::info!(
        "[CPU] {}: Sensor query returned {} temperature rows",
        source_name,
        results.len()
    );

    let mut preferred: Option<f32> = None;
    let mut core_temps: Vec<f32> = Vec::new();

    for row in &results {
        let identifier = match row.get("Identifier") {
            Some(Variant::String(s)) => s.to_lowercase(),
            _ => continue,
        };
        // Only consider CPU sensors — LHM also exposes GPU/mobo temps here.
        if !(identifier.contains("/cpu/")
            || identifier.contains("/intelcpu/")
            || identifier.contains("/amdcpu/"))
        {
            continue;
        }

        let value = match row.get("Value") {
            Some(Variant::R4(v)) => *v,
            Some(Variant::R8(v)) => *v as f32,
            _ => continue,
        };
        if !(value > 0.0 && value < 150.0) {
            continue;
        }

        let name = match row.get("Name") {
            Some(Variant::String(s)) => s.to_lowercase(),
            _ => String::new(),
        };

        // Prefer aggregate sensors (in priority order)
        if preferred.is_none()
            && (name.contains("package")
                || name.contains("tdie")
                || name.contains("ccd average")
                || name.contains("cpu total"))
        {
            preferred = Some(value);
        }

        if name.contains("core") {
            core_temps.push(value);
        }
    }

    log::info!(
        "[CPU] {}: matched {} CPU core temps, preferred aggregate = {:?}",
        source_name,
        core_temps.len(),
        preferred
    );

    if let Some(temp) = preferred {
        log::info!("[CPU] Temperature from {} aggregate: {:.1}°C", source_name, temp);
        return Some(temp);
    }
    if !core_temps.is_empty() {
        let avg = core_temps.iter().sum::<f32>() / core_temps.len() as f32;
        log::info!(
            "[CPU] Temperature from {} (avg of {} cores): {:.1}°C",
            source_name,
            core_temps.len(),
            avg
        );
        return Some(avg);
    }
    None
}

/// Try to read CPU temperature from standard Windows WMI namespaces.
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
                    log::info!(
                        "[CPU] MSAcpi_ThermalZone returned {} row(s)",
                        results.len()
                    );
                    for (i, result) in results.iter().enumerate() {
                        let raw = result.get("CurrentTemperature");
                        log::info!("[CPU] MSAcpi_ThermalZone row {}: raw CurrentTemperature = {:?}", i, raw);
                        let raw_temp = match raw {
                            Some(Variant::UI4(n)) => Some(*n as f32),
                            Some(Variant::UI2(n)) => Some(*n as f32),
                            Some(Variant::I4(n)) => Some(*n as f32),
                            _ => None,
                        };
                        if let Some(tenths_kelvin) = raw_temp {
                            let celsius = (tenths_kelvin / 10.0) - 273.15;
                            log::info!(
                                "[CPU] MSAcpi_ThermalZone row {} -> {:.1}°C (raw {} tenths-K)",
                                i, celsius, tenths_kelvin
                            );
                            if celsius > 0.0 && celsius < 150.0 {
                                log::info!("[CPU] Temperature from MSAcpi_ThermalZone: {:.1}°C", celsius);
                                return Some(celsius);
                            } else {
                                log::info!(
                                    "[CPU] MSAcpi_ThermalZone row {} out of range ({:.1}°C), discarding",
                                    i, celsius
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    log::info!("[CPU] MSAcpi_ThermalZoneTemperature query failed (needs admin?): {}", e);
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
                    log::info!(
                        "[CPU] ThermalZoneInformation returned {} row(s)",
                        results.len()
                    );
                    for (i, result) in results.iter().enumerate() {
                        let raw = result.get("Temperature");
                        log::info!("[CPU] ThermalZoneInformation row {}: raw Temperature = {:?}", i, raw);
                        let kelvin = match raw {
                            Some(Variant::UI4(n)) => Some(*n as f32),
                            Some(Variant::UI2(n)) => Some(*n as f32),
                            Some(Variant::I4(n)) => Some(*n as f32),
                            Some(Variant::UI8(n)) => Some(*n as f32),
                            _ => None,
                        };
                        if let Some(k) = kelvin {
                            let celsius = k - 273.15;
                            log::info!(
                                "[CPU] ThermalZoneInformation row {} -> {:.1}°C (raw {} K)",
                                i, celsius, k
                            );
                            if celsius > 0.0 && celsius < 150.0 {
                                log::info!("[CPU] Temperature from ThermalZoneInformation: {:.1}°C", celsius);
                                return Some(celsius);
                            } else {
                                log::info!(
                                    "[CPU] ThermalZoneInformation row {} out of range ({:.1}°C), discarding",
                                    i, celsius
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    log::info!("[CPU] ThermalZoneInformation query failed: {}", e);
                }
            }
        }
    }

    log::warn!(
        "[CPU] No CPU temperature available from any WMI source. \
        Check log entries above — sysinfo, LHM, OHM, MSAcpi, and ThermalZoneInformation \
        were all attempted. This is a known limitation on consumer Windows hardware \
        without a kernel-mode driver; Phase 2 will address this with a bundled hwmon helper."
    );
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::hwmon_client::HwmonSnapshot;

    #[test]
    fn collect_uses_hwmon_snapshot_when_provided() {
        let sys = sysinfo::System::new_all();
        let snap = Some(HwmonSnapshot {
            cpu_package_c: Some(48.5),
            cpu_core_avg_c: Some(46.0),
        });
        let data = collect(&sys, snap.as_ref());
        assert_eq!(data.temperature, Some(48.5),
            "hwmon snapshot must win over WMI fallback");
    }

    #[test]
    fn collect_falls_back_when_snapshot_is_none() {
        // Without a snapshot we exercise the existing WMI chain. The result
        // depends on hardware so we only assert that the function doesn't
        // panic and returns a value with the expected shape.
        let sys = sysinfo::System::new_all();
        let data = collect(&sys, None);
        assert!(data.usage_percent >= 0.0);
    }
}
