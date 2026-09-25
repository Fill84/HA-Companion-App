use std::collections::{HashMap, HashSet};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sysinfo::System;

use super::temperature::TemperatureReader;
use super::{battery, catalog, cpu, disk, gpu, memory, network, system_info};

fn assign_sensor_suffixes(
    map: &mut HashMap<String, String>,
    category: &str,
    physical_ids: &[Option<String>],
    preserve_single_legacy: bool,
) -> Vec<Option<String>> {
    let prefix = format!("{category}:");
    let mut counts = HashMap::new();
    for id in physical_ids.iter().filter_map(|id| id.as_deref()) {
        *counts.entry(id).or_insert(0usize) += 1;
    }
    let mut used: HashSet<String> = map
        .iter()
        .filter(|(key, _)| key.starts_with(&prefix))
        .map(|(_, suffix)| suffix.clone())
        .collect();
    let had_assignment = !used.is_empty();
    physical_ids
        .iter()
        .map(|physical_id| {
            let Some(id) = physical_id.as_deref().filter(|id| !id.is_empty()) else {
                return (preserve_single_legacy && physical_ids.len() == 1 && !had_assignment)
                    .then(String::new);
            };
            if counts.get(id) != Some(&1) {
                return None;
            }
            let key = format!("{prefix}{id}");
            if let Some(suffix) = map.get(&key) {
                let aliases = map
                    .iter()
                    .filter(|(other_key, other_suffix)| {
                        other_key.starts_with(&prefix) && *other_suffix == suffix
                    })
                    .count();
                return (aliases == 1).then(|| suffix.clone());
            }
            let suffix = if preserve_single_legacy && physical_ids.len() == 1 && !had_assignment {
                String::new()
            } else {
                let mut index = 1usize;
                loop {
                    let candidate = format!("_stable_{index}");
                    if !used.contains(&candidate) {
                        break candidate;
                    }
                    index += 1;
                }
            };
            used.insert(suffix.clone());
            map.insert(key, suffix.clone());
            Some(suffix)
        })
        .collect()
}

fn valid_legacy_gpu_suffix(suffix: &str) -> bool {
    suffix.is_empty()
        || (suffix.len() <= 32
            && suffix.starts_with('_')
            && suffix[1..]
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'))
}

fn add_legacy_gpu_aliases(
    sensors: &mut Vec<SensorValue>,
    first: usize,
    canonical_suffix: &str,
    physical_id: Option<&str>,
    aliases: &HashMap<String, Vec<String>>,
    reserved_suffixes: &HashSet<String>,
) {
    let Some(physical_id) = physical_id else {
        return;
    };
    let key = format!("gpu:{physical_id}");
    let Some(requested) = aliases.get(&key) else {
        return;
    };
    let originals: Vec<_> = sensors[first..].to_vec();
    for alias in requested {
        if alias == canonical_suffix
            || !valid_legacy_gpu_suffix(alias)
            || reserved_suffixes.contains(alias)
            || aliases
                .iter()
                .filter(|(candidate, suffixes)| {
                    candidate.starts_with("gpu:") && suffixes.contains(alias)
                })
                .count()
                != 1
        {
            continue;
        }
        for original in &originals {
            let Some(base) = original.unique_id.strip_suffix(canonical_suffix) else {
                continue;
            };
            let unique_id = format!("{base}{alias}");
            if sensors.iter().any(|sensor| sensor.unique_id == unique_id) {
                continue;
            }
            let mut copy = original.clone();
            copy.unique_id = unique_id;
            sensors.push(copy);
        }
    }
}

fn rounded(value: f64, decimal_places: i32) -> f64 {
    let factor = 10_f64.powi(decimal_places);
    (value * factor).round() / factor
}

fn disk_attributes(partition: &disk::PartitionData) -> HashMap<String, serde_json::Value> {
    HashMap::from([
        (
            "total_gb".into(),
            serde_json::json!(rounded(partition.total_bytes as f64 / 1_000_000_000.0, 1)),
        ),
        (
            "used_gb".into(),
            serde_json::json!(rounded(partition.used_bytes as f64 / 1_000_000_000.0, 1)),
        ),
        ("filesystem".into(), serde_json::json!(partition.filesystem)),
        ("disk_type".into(), serde_json::json!(partition.disk_type)),
    ])
}

/// Format a UNIX timestamp (seconds since 1970-01-01 UTC) as an RFC3339 string
/// with a `+00:00` offset suffix. Returns `None` for the failure-mode value 0,
/// so the Last Boot sensor can be omitted entirely rather than reporting 1970.
pub(crate) fn format_boot_time(timestamp: u64) -> Option<String> {
    if timestamp == 0 {
        return None;
    }
    let dt = Utc.timestamp_opt(timestamp as i64, 0).single()?;
    Some(dt.to_rfc3339_opts(SecondsFormat::Secs, false))
}

/// Build the Last Boot SensorValue, or None when the boot time is unknown.
///
/// State is RFC3339; the HA integration converts it to a datetime object.
pub(crate) fn build_last_boot_sensor(boot_time: u64) -> Option<SensorValue> {
    let iso = format_boot_time(boot_time)?;
    let readable = format_boot_time_readable(boot_time)?;

    let mut attributes = HashMap::new();
    attributes.insert("boot_timestamp".into(), serde_json::json!(boot_time));
    attributes.insert("iso_utc".into(), serde_json::json!(iso));
    attributes.insert("display_utc".into(), serde_json::json!(readable));

    Some(SensorValue {
        unique_id: "last_boot".into(),
        name: "Last Boot".into(),
        state: serde_json::json!(iso),
        sensor_type: "sensor".into(),
        device_class: Some("timestamp".into()),
        unit_of_measurement: None,
        state_class: None,
        icon: Some("mdi:restart".into()),
        attributes,
        update_at_interval: false,
    })
}

/// Format a UNIX timestamp as a human-readable UTC date+time string,
/// e.g. "2026-05-26 12:00 UTC". Returns None for the failure-mode value 0.
pub(crate) fn format_boot_time_readable(timestamp: u64) -> Option<String> {
    if timestamp == 0 {
        return None;
    }
    let dt = Utc.timestamp_opt(timestamp as i64, 0).single()?;
    Some(dt.format("%Y-%m-%d %H:%M UTC").to_string())
}

/// Build the SensorValue for System Uptime.
///
/// Numeric seconds preserve HA duration semantics and statistics.
pub(crate) fn build_uptime_sensor(uptime_seconds: u64) -> SensorValue {
    let days = uptime_seconds / 86400;
    let hours = uptime_seconds / 3600;
    let minutes = (uptime_seconds % 3600) / 60;

    let human = if days > 0 {
        format!("{}d {}h {}m", days, hours - days * 24, minutes)
    } else {
        format!("{}h {}m", hours, minutes)
    };

    let mut attributes = HashMap::new();
    attributes.insert("uptime_seconds".into(), serde_json::json!(uptime_seconds));
    attributes.insert("days".into(), serde_json::json!(days));
    attributes.insert("hours".into(), serde_json::json!(hours));
    attributes.insert("minutes".into(), serde_json::json!(minutes));
    attributes.insert("human_readable".into(), serde_json::json!(human));

    SensorValue {
        unique_id: "system_uptime".into(),
        name: "System Uptime".into(),
        state: serde_json::json!(uptime_seconds),
        sensor_type: "sensor".into(),
        device_class: Some("duration".into()),
        unit_of_measurement: Some("s".into()),
        state_class: Some("measurement".into()),
        icon: Some("mdi:clock-outline".into()),
        attributes,
        update_at_interval: true,
    }
}

/// Represents a single sensor value for HA
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorValue {
    pub unique_id: String,
    pub name: String,
    pub state: serde_json::Value,
    pub sensor_type: String, // "sensor" or "binary_sensor"
    pub device_class: Option<String>,
    pub unit_of_measurement: Option<String>,
    pub state_class: Option<String>,
    pub icon: Option<String>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub update_at_interval: bool,
}

/// Collects all sensor data and formats for HA
pub struct SensorCollector {
    sys: System,
    enabled_sensors: HashMap<String, bool>,
    identity_map: HashMap<String, String>,
    legacy_gpu_aliases: HashMap<String, Vec<String>>,
    temperature: TemperatureReader,
}

impl SensorCollector {
    pub fn new(
        enabled_sensors: &HashMap<String, bool>,
        identity_map: &HashMap<String, String>,
        legacy_gpu_aliases: &HashMap<String, Vec<String>>,
    ) -> Self {
        let sys = System::new_with_specifics(
            sysinfo::RefreshKind::new()
                .with_cpu(sysinfo::CpuRefreshKind::everything())
                .with_memory(sysinfo::MemoryRefreshKind::everything()),
        );

        Self {
            sys,
            enabled_sensors: enabled_sensors.clone(),
            identity_map: identity_map.clone(),
            legacy_gpu_aliases: legacy_gpu_aliases.clone(),
            temperature: TemperatureReader::new(),
        }
    }

    pub fn identity_map(&self) -> HashMap<String, String> {
        self.identity_map.clone()
    }

    fn is_enabled(&self, sensor_id: &str) -> bool {
        *self.enabled_sensors.get(sensor_id).unwrap_or(&true)
    }

    fn retain_enabled_readings(&self, sensors: &mut Vec<SensorValue>) {
        sensors.retain(|sensor| self.is_enabled(&format!("sensor:{}", sensor.unique_id)));
    }

    /// Collect all sensors (both static and dynamic) — used at startup
    pub fn collect_all(&mut self) -> Vec<SensorValue> {
        let mut sensors = Vec::new();

        sensors.extend(self.collect_static());
        sensors.extend(self.collect_dynamic());

        self.retain_enabled_readings(&mut sensors);
        sensors
    }

    /// Collect only dynamic sensors — used at interval
    pub fn collect_dynamic(&mut self) -> Vec<SensorValue> {
        let mut sensors = Vec::new();

        let mut cpu_refresh = sysinfo::CpuRefreshKind::new();
        if self.is_enabled("cpu_usage") {
            cpu_refresh = cpu_refresh.with_cpu_usage();
        }
        if self.is_enabled("cpu_frequency") {
            cpu_refresh = cpu_refresh.with_frequency();
        }
        if self.is_enabled("cpu_usage") || self.is_enabled("cpu_frequency") {
            self.sys.refresh_cpu_specifics(cpu_refresh);
        }
        if self.is_enabled("memory_usage")
            || self.is_enabled("memory_used")
            || self.is_enabled("swap_usage")
        {
            self.sys.refresh_memory();
        }
        if self.is_enabled("process_count") {
            self.sys.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::All,
                true,
                sysinfo::ProcessRefreshKind::new(),
            );
        }

        // CPU sensors (dynamic) — collect once, reuse
        let cpu_enabled = self.is_enabled("cpu_usage")
            || self.is_enabled("cpu_frequency")
            || self.is_enabled("cpu_temperature");
        if cpu_enabled {
            let cpu_data = cpu::collect(&self.sys);

            if self.is_enabled("cpu_usage") {
                sensors.push(SensorValue {
                    unique_id: "cpu_usage".into(),
                    name: "CPU Usage".into(),
                    state: serde_json::json!(rounded(cpu_data.usage_percent as f64, 1)),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: Some("%".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:cpu-64-bit".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }

            if self.is_enabled("cpu_frequency") {
                sensors.push(SensorValue {
                    unique_id: "cpu_frequency".into(),
                    name: "CPU Frequency".into(),
                    state: serde_json::json!(cpu_data.frequency_mhz),
                    sensor_type: "sensor".into(),
                    device_class: Some("frequency".into()),
                    unit_of_measurement: Some("MHz".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:speedometer".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }

            if self.is_enabled("cpu_temperature") {
                let reading = self.temperature.read();
                sensors.push(SensorValue {
                    unique_id: "cpu_temperature".into(),
                    name: "CPU Temperature".into(),
                    state: serde_json::json!(reading.value),
                    sensor_type: "sensor".into(),
                    device_class: Some("temperature".into()),
                    unit_of_measurement: Some("°C".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:thermometer".into()),
                    attributes: reading.attributes(),
                    update_at_interval: true,
                });
            }
        }

        // Memory sensors (dynamic) — collect once, reuse
        let mem_enabled = self.is_enabled("memory_usage")
            || self.is_enabled("memory_used")
            || self.is_enabled("swap_usage");
        if mem_enabled {
            let mem_data = memory::collect(&self.sys);

            if self.is_enabled("memory_usage") {
                sensors.push(SensorValue {
                    unique_id: "memory_usage".into(),
                    name: "Memory Usage".into(),
                    state: serde_json::json!(rounded(mem_data.usage_percent as f64, 1)),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: Some("%".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:memory".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }

            if self.is_enabled("memory_used") {
                sensors.push(SensorValue {
                    unique_id: "memory_used".into(),
                    name: "Memory Used".into(),
                    state: serde_json::json!(rounded(mem_data.used_gb, 2)),
                    sensor_type: "sensor".into(),
                    device_class: Some("data_size".into()),
                    unit_of_measurement: Some("GB".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:memory".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }

            // Swap sensors
            if self.is_enabled("swap_usage") && mem_data.swap_total_bytes > 0 {
                let swap_usage_pct = if mem_data.swap_total_bytes > 0 {
                    (mem_data.swap_used_bytes as f32 / mem_data.swap_total_bytes as f32) * 100.0
                } else {
                    0.0
                };
                let swap_used_gb = mem_data.swap_used_bytes as f64 / 1_000_000_000.0;
                let swap_total_gb = mem_data.swap_total_bytes as f64 / 1_000_000_000.0;

                sensors.push(SensorValue {
                    unique_id: "swap_usage".into(),
                    name: "Swap Usage".into(),
                    state: serde_json::json!(rounded(swap_usage_pct as f64, 1)),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: Some("%".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:swap-horizontal".into()),
                    attributes: {
                        let mut attrs = HashMap::new();
                        attrs.insert(
                            "swap_used_gb".into(),
                            serde_json::json!(rounded(swap_used_gb, 2)),
                        );
                        attrs.insert(
                            "swap_total_gb".into(),
                            serde_json::json!(rounded(swap_total_gb, 1)),
                        );
                        attrs
                    },
                    update_at_interval: true,
                });
            }
        }

        // Disk sensors (dynamic)
        if self.is_enabled("disk_usage") {
            let disk_data = disk::collect();
            let identities: Vec<_> = disk_data
                .partitions
                .iter()
                .map(|disk| disk.physical_id.clone())
                .collect();
            let suffixes =
                assign_sensor_suffixes(&mut self.identity_map, "disk", &identities, false);
            for (partition, suffix) in disk_data.partitions.iter().zip(suffixes) {
                let Some(suffix) = suffix else { continue };
                sensors.push(SensorValue {
                    unique_id: format!("disk_usage{suffix}"),
                    name: format!("Disk Usage {}", partition.mount_point),
                    state: serde_json::json!(rounded(partition.usage_percent as f64, 1)),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: Some("%".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:harddisk".into()),
                    attributes: disk_attributes(partition),
                    update_at_interval: true,
                });
            }
        }

        // GPU sensors (dynamic)
        if self.is_enabled("gpu") {
            let gpu_data = gpu::collect();
            let identities: Vec<_> = gpu_data
                .gpus
                .iter()
                .map(|gpu| gpu.physical_id.clone())
                .collect();
            let suffixes = assign_sensor_suffixes(&mut self.identity_map, "gpu", &identities, true);
            let reserved_suffixes: HashSet<_> = suffixes.iter().flatten().cloned().collect();
            for (i, (gpu_info, suffix)) in gpu_data.gpus.iter().zip(suffixes).enumerate() {
                let Some(suffix) = suffix else { continue };
                let first = sensors.len();

                if let Some(usage) = gpu_info.usage_percent {
                    sensors.push(SensorValue {
                        unique_id: format!("gpu_usage{}", suffix),
                        name: format!(
                            "GPU Usage{}",
                            if suffix.is_empty() {
                                "".to_string()
                            } else {
                                format!(" {}", i)
                            }
                        ),
                        state: serde_json::json!(rounded(usage as f64, 1)),
                        sensor_type: "sensor".into(),
                        device_class: None,
                        unit_of_measurement: Some("%".into()),
                        state_class: Some("measurement".into()),
                        icon: Some("mdi:expansion-card".into()),
                        attributes: HashMap::new(),
                        update_at_interval: true,
                    });
                }

                if let Some(temp) = gpu_info.temperature {
                    sensors.push(SensorValue {
                        unique_id: format!("gpu_temperature{}", suffix),
                        name: format!(
                            "GPU Temperature{}",
                            if suffix.is_empty() {
                                "".to_string()
                            } else {
                                format!(" {}", i)
                            }
                        ),
                        state: serde_json::json!(rounded(temp as f64, 1)),
                        sensor_type: "sensor".into(),
                        device_class: Some("temperature".into()),
                        unit_of_measurement: Some("°C".into()),
                        state_class: Some("measurement".into()),
                        icon: Some("mdi:thermometer".into()),
                        attributes: HashMap::new(),
                        update_at_interval: true,
                    });
                }

                if let Some(vram_used) = gpu_info.vram_used_mb {
                    sensors.push(SensorValue {
                        unique_id: format!("gpu_vram_used{}", suffix),
                        name: format!(
                            "GPU VRAM Used{}",
                            if suffix.is_empty() {
                                "".to_string()
                            } else {
                                format!(" {}", i)
                            }
                        ),
                        state: serde_json::json!(rounded(vram_used as f64, 0)),
                        sensor_type: "sensor".into(),
                        device_class: Some("data_size".into()),
                        unit_of_measurement: Some("MB".into()),
                        state_class: Some("measurement".into()),
                        icon: Some("mdi:expansion-card-variant".into()),
                        attributes: HashMap::new(),
                        update_at_interval: true,
                    });
                }
                add_legacy_gpu_aliases(
                    &mut sensors,
                    first,
                    &suffix,
                    gpu_info.physical_id.as_deref(),
                    &self.legacy_gpu_aliases,
                    &reserved_suffixes,
                );
            }
        }

        // Network sensors (dynamic)
        if self.is_enabled("network") {
            let net_data = network::collect();
            let identities: Vec<_> = net_data
                .interfaces
                .iter()
                .map(|interface| interface.physical_id.clone())
                .collect();
            let suffixes =
                assign_sensor_suffixes(&mut self.identity_map, "network", &identities, false);
            for (iface, suffix) in net_data.interfaces.iter().zip(suffixes) {
                let Some(suffix) = suffix else { continue };
                sensors.push(SensorValue {
                    unique_id: format!("network_rx{suffix}"),
                    name: format!("Network RX {}", iface.name),
                    state: serde_json::json!(iface.received_bytes),
                    sensor_type: "sensor".into(),
                    device_class: Some("data_size".into()),
                    unit_of_measurement: Some("B".into()),
                    state_class: Some("total_increasing".into()),
                    icon: Some("mdi:download-network".into()),
                    attributes: {
                        let mut attrs = HashMap::new();
                        attrs.insert("mac_address".into(), serde_json::json!(iface.mac_address));
                        attrs.insert("ip_addresses".into(), serde_json::json!(iface.ip_addresses));
                        attrs
                    },
                    update_at_interval: true,
                });

                sensors.push(SensorValue {
                    unique_id: format!("network_tx{suffix}"),
                    name: format!("Network TX {}", iface.name),
                    state: serde_json::json!(iface.transmitted_bytes),
                    sensor_type: "sensor".into(),
                    device_class: Some("data_size".into()),
                    unit_of_measurement: Some("B".into()),
                    state_class: Some("total_increasing".into()),
                    icon: Some("mdi:upload-network".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }
        }

        // Battery sensors (dynamic)
        if self.is_enabled("battery") {
            let battery_data = battery::collect();
            let identities: Vec<_> = battery_data
                .batteries
                .iter()
                .map(|bat| bat.physical_id.clone())
                .collect();
            let suffixes =
                assign_sensor_suffixes(&mut self.identity_map, "battery", &identities, true);
            for (i, (bat, suffix)) in battery_data.batteries.iter().zip(suffixes).enumerate() {
                let Some(suffix) = suffix else { continue };

                sensors.push(SensorValue {
                    unique_id: format!("battery_level{}", suffix),
                    name: format!(
                        "Battery Level{}",
                        if suffix.is_empty() {
                            "".to_string()
                        } else {
                            format!(" {}", i)
                        }
                    ),
                    state: serde_json::json!(rounded(bat.percentage as f64, 0)),
                    sensor_type: "sensor".into(),
                    device_class: Some("battery".into()),
                    unit_of_measurement: Some("%".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:battery".into()),
                    attributes: {
                        let mut attrs = HashMap::new();
                        attrs.insert("state".into(), serde_json::json!(bat.state));
                        if let Some(health) = bat.state_of_health {
                            attrs.insert(
                                "state_of_health".into(),
                                serde_json::json!(format!("{:.0}%", health)),
                            );
                        }
                        if let Some(cycles) = bat.cycle_count {
                            attrs.insert("cycle_count".into(), serde_json::json!(cycles));
                        }
                        attrs
                    },
                    update_at_interval: true,
                });

                sensors.push(SensorValue {
                    unique_id: format!("battery_charging{}", suffix),
                    name: format!(
                        "Battery Charging{}",
                        if suffix.is_empty() {
                            "".to_string()
                        } else {
                            format!(" {}", i)
                        }
                    ),
                    state: serde_json::json!(bat.is_charging),
                    sensor_type: "binary_sensor".into(),
                    device_class: Some("battery_charging".into()),
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:battery-charging".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }
        }

        // System uptime & process count (dynamic)
        if self.is_enabled("system_uptime") || self.is_enabled("process_count") {
            let dyn_info = system_info::collect_dynamic(&self.sys);

            if self.is_enabled("system_uptime") {
                sensors.push(build_uptime_sensor(dyn_info.uptime_seconds));
            }

            if self.is_enabled("process_count") {
                sensors.push(SensorValue {
                    unique_id: "process_count".into(),
                    name: "Process Count".into(),
                    state: serde_json::json!(dyn_info.process_count),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: Some("processes".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:format-list-numbered".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }
        }

        self.retain_enabled_readings(&mut sensors);
        sensors
    }

    /// Collect static sensors — only at startup
    pub fn collect_static(&mut self) -> Vec<SensorValue> {
        let mut sensors = Vec::new();

        // CPU model (static)
        if self.is_enabled("cpu_model") {
            let cpu_data = cpu::collect(&self.sys);
            sensors.push(SensorValue {
                unique_id: "cpu_model".into(),
                name: "CPU Model".into(),
                state: serde_json::json!(cpu_data.model),
                sensor_type: "sensor".into(),
                device_class: None,
                unit_of_measurement: None,
                state_class: None,
                icon: Some("mdi:cpu-64-bit".into()),
                attributes: {
                    let mut attrs = HashMap::new();
                    attrs.insert("core_count".into(), serde_json::json!(cpu_data.core_count));
                    attrs.insert(
                        "logical_core_count".into(),
                        serde_json::json!(cpu_data.logical_core_count),
                    );
                    attrs
                },
                update_at_interval: false,
            });
        }

        // System info (static)
        let sys_info = system_info::collect();

        if self.is_enabled("os_version") {
            sensors.push(SensorValue {
                unique_id: "os_version".into(),
                name: "OS Version".into(),
                state: serde_json::json!(format!("{} {}", sys_info.os_name, sys_info.os_version)),
                sensor_type: "sensor".into(),
                device_class: None,
                unit_of_measurement: None,
                state_class: None,
                icon: Some("mdi:monitor".into()),
                attributes: {
                    let mut attrs = HashMap::new();
                    attrs.insert("os_name".into(), serde_json::json!(sys_info.os_name));
                    attrs.insert("os_version".into(), serde_json::json!(sys_info.os_version));
                    attrs
                },
                update_at_interval: false,
            });
        }

        if self.is_enabled("hostname") {
            sensors.push(SensorValue {
                unique_id: "hostname".into(),
                name: "Hostname".into(),
                state: serde_json::json!(sys_info.hostname),
                sensor_type: "sensor".into(),
                device_class: None,
                unit_of_measurement: None,
                state_class: None,
                icon: Some("mdi:desktop-tower".into()),
                attributes: HashMap::new(),
                update_at_interval: false,
            });
        }

        if self.is_enabled("motherboard") {
            if let (Some(ref mfr), Some(ref model)) = (
                &sys_info.motherboard_manufacturer,
                &sys_info.motherboard_model,
            ) {
                sensors.push(SensorValue {
                    unique_id: "motherboard".into(),
                    name: "Motherboard".into(),
                    state: serde_json::json!(format!("{} {}", mfr, model)),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:expansion-card".into()),
                    attributes: {
                        let mut attrs = HashMap::new();
                        attrs.insert("manufacturer".into(), serde_json::json!(mfr));
                        attrs.insert("model".into(), serde_json::json!(model));
                        attrs
                    },
                    update_at_interval: false,
                });
            }
        }

        // BIOS sensors (static)
        if self.is_enabled("bios_version") {
            if let Some(ref bios) = sys_info.bios_version {
                let mut attrs = HashMap::new();
                if let Some(ref vendor) = sys_info.bios_vendor {
                    attrs.insert("vendor".into(), serde_json::json!(vendor));
                }
                if let Some(ref date) = sys_info.bios_release_date {
                    attrs.insert("release_date".into(), serde_json::json!(date));
                }

                sensors.push(SensorValue {
                    unique_id: "bios_version".into(),
                    name: "BIOS Version".into(),
                    state: serde_json::json!(bios),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:chip".into()),
                    attributes: attrs,
                    update_at_interval: false,
                });
            }
        }

        if self.is_enabled("bios_vendor") {
            if let Some(ref vendor) = sys_info.bios_vendor {
                sensors.push(SensorValue {
                    unique_id: "bios_vendor".into(),
                    name: "BIOS Vendor".into(),
                    state: serde_json::json!(vendor),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:chip".into()),
                    attributes: HashMap::new(),
                    update_at_interval: false,
                });
            }
        }

        if self.is_enabled("bios_date") {
            if let Some(ref date) = sys_info.bios_release_date {
                sensors.push(SensorValue {
                    unique_id: "bios_date".into(),
                    name: "BIOS Date".into(),
                    state: serde_json::json!(date),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:calendar".into()),
                    attributes: HashMap::new(),
                    update_at_interval: false,
                });
            }
        }

        // Last boot time (static)
        if self.is_enabled("last_boot") {
            if let Some(sensor) = build_last_boot_sensor(sys_info.boot_time) {
                sensors.push(sensor);
            } else {
                log::warn!("[SystemInfo] boot_time is 0 — skipping Last Boot sensor");
            }
        }

        // Logged-in user (static)
        if self.is_enabled("logged_in_user") {
            if let Some(ref user) = sys_info.logged_in_user {
                sensors.push(SensorValue {
                    unique_id: "logged_in_user".into(),
                    name: "Logged In User".into(),
                    state: serde_json::json!(user),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:account".into()),
                    attributes: HashMap::new(),
                    update_at_interval: false,
                });
            }
        }

        // Display info (static)
        if self.is_enabled("display") {
            let identities: Vec<_> = sys_info
                .displays
                .iter()
                .map(|display| display.physical_id.clone())
                .collect();
            let suffixes =
                assign_sensor_suffixes(&mut self.identity_map, "display", &identities, true);
            for (i, (display, suffix)) in sys_info.displays.iter().zip(suffixes).enumerate() {
                let Some(suffix) = suffix else { continue };

                sensors.push(SensorValue {
                    unique_id: format!("display_resolution{}", suffix),
                    name: format!(
                        "Display Resolution{}",
                        if suffix.is_empty() {
                            "".to_string()
                        } else {
                            format!(" {}", i + 1)
                        }
                    ),
                    state: serde_json::json!(display.resolution),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:monitor".into()),
                    attributes: {
                        let mut attrs = HashMap::new();
                        attrs.insert("adapter".into(), serde_json::json!(display.name));
                        if let Some(hz) = display.refresh_rate_hz {
                            attrs.insert("refresh_rate_hz".into(), serde_json::json!(hz));
                        }
                        attrs
                    },
                    update_at_interval: false,
                });
            }
        }

        // GPU model (static)
        if self.is_enabled("gpu") {
            let gpu_data = gpu::collect();
            let identities: Vec<_> = gpu_data
                .gpus
                .iter()
                .map(|gpu| gpu.physical_id.clone())
                .collect();
            let suffixes = assign_sensor_suffixes(&mut self.identity_map, "gpu", &identities, true);
            let reserved_suffixes: HashSet<_> = suffixes.iter().flatten().cloned().collect();
            for (i, (gpu_info, suffix)) in gpu_data.gpus.iter().zip(suffixes).enumerate() {
                let Some(suffix) = suffix else { continue };
                let first = sensors.len();

                sensors.push(SensorValue {
                    unique_id: format!("gpu_model{}", suffix),
                    name: format!(
                        "GPU Model{}",
                        if suffix.is_empty() {
                            "".to_string()
                        } else {
                            format!(" {}", i)
                        }
                    ),
                    state: serde_json::json!(gpu_info.name),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: None,
                    state_class: None,
                    icon: Some("mdi:expansion-card".into()),
                    attributes: {
                        let mut attrs = HashMap::new();
                        attrs.insert("vendor".into(), serde_json::json!(gpu_info.vendor));
                        if let Some(ref driver) = gpu_info.driver_version {
                            attrs.insert("driver_version".into(), serde_json::json!(driver));
                        }
                        if let Some(vram) = gpu_info.vram_total_mb {
                            attrs.insert("vram_total_mb".into(), serde_json::json!(vram));
                        }
                        attrs
                    },
                    update_at_interval: false,
                });
                add_legacy_gpu_aliases(
                    &mut sensors,
                    first,
                    &suffix,
                    gpu_info.physical_id.as_deref(),
                    &self.legacy_gpu_aliases,
                    &reserved_suffixes,
                );
            }
        }

        // RAM total (static)
        if self.is_enabled("memory_total") {
            let mem_data = memory::collect(&self.sys);
            sensors.push(SensorValue {
                unique_id: "memory_total".into(),
                name: "Memory Total".into(),
                state: serde_json::json!(rounded(mem_data.total_gb, 1)),
                sensor_type: "sensor".into(),
                device_class: Some("data_size".into()),
                unit_of_measurement: Some("GB".into()),
                state_class: None,
                icon: Some("mdi:memory".into()),
                attributes: HashMap::new(),
                update_at_interval: false,
            });
        }

        sensors
    }

    /// Get list of all possible sensors and their enabled status
    pub fn get_sensor_list(&mut self) -> Vec<SensorListItem> {
        // Probe with every group enabled so disabled groups remain discoverable.
        // Keep live preferences and provider state untouched during discovery.
        let mut probe = Self::new(
            &HashMap::new(),
            &self.identity_map,
            &self.legacy_gpu_aliases,
        );
        let readings = probe.collect_all();
        self.identity_map = probe.identity_map;
        let mut seen = HashSet::new();
        let mut list = Vec::new();
        for choice in catalog::SENSOR_CHOICES {
            list.push(SensorListItem {
                id: choice.id.to_string(),
                name: choice.name_en.to_string(),
                name_nl: choice.name_nl.to_string(),
                enabled: self.is_enabled(choice.id),
                updates_at_interval: choice.updates_at_interval,
                group_id: None,
            });
            for sensor in readings
                .iter()
                .filter(|sensor| sensor_group(&sensor.unique_id) == Some(choice.id))
            {
                if !seen.insert(sensor.unique_id.clone()) {
                    continue;
                }
                list.push(SensorListItem {
                    id: format!("sensor:{}", sensor.unique_id),
                    name: sensor.name.clone(),
                    name_nl: sensor.name.clone(),
                    enabled: self.is_enabled(&format!("sensor:{}", sensor.unique_id)),
                    updates_at_interval: sensor.update_at_interval,
                    group_id: Some(choice.id.to_string()),
                });
            }
        }
        list
    }

    /// Update enabled sensors map
    pub fn set_enabled_sensors(&mut self, enabled: HashMap<String, bool>) {
        if enabled.get("cpu_temperature") == Some(&false) {
            self.temperature.suspend();
        }
        self.enabled_sensors = enabled;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorListItem {
    pub id: String,
    pub name: String,
    pub name_nl: String,
    pub enabled: bool,
    pub updates_at_interval: bool,
    pub group_id: Option<String>,
}

fn sensor_group(id: &str) -> Option<&'static str> {
    if let Some(choice) = catalog::SENSOR_CHOICES
        .iter()
        .find(|choice| choice.id == id)
    {
        return Some(choice.id);
    }
    if id.starts_with("disk_usage") {
        Some("disk_usage")
    } else if id.starts_with("gpu_") {
        Some("gpu")
    } else if id.starts_with("network_") {
        Some("network")
    } else if id.starts_with("battery_") {
        Some("battery")
    } else if id.starts_with("display_resolution") {
        Some("display")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sensor_choices_keep_ids_and_localized_labels() {
        let (choices, readings) = tokio::task::spawn_blocking(|| {
            let mut collector =
                SensorCollector::new(&HashMap::new(), &HashMap::new(), &HashMap::new());
            (collector.get_sensor_list(), collector.collect_all())
        })
        .await
        .unwrap();
        assert_eq!(
            choices
                .iter()
                .filter(|choice| choice.group_id.is_none())
                .count(),
            23
        );
        assert_eq!(choices[0].id, "cpu_usage");
        assert_eq!(choices[0].name, "CPU Usage");
        assert_eq!(choices[0].name_nl, "CPU Gebruik");
        assert!(choices[0].enabled);
        assert!(choices[0].updates_at_interval);
        let display = choices
            .iter()
            .find(|choice| choice.id == "display")
            .unwrap();
        assert!(!display.updates_at_interval);
        assert!(choices.iter().any(|choice| choice.id == "sensor:cpu_usage"));
        for reading in readings {
            assert!(
                sensor_group(&reading.unique_id).is_some(),
                "unlisted reading: {}",
                reading.unique_id
            );
            assert!(choices
                .iter()
                .any(|choice| choice.id == format!("sensor:{}", reading.unique_id)));
        }
    }

    #[tokio::test]
    async fn individual_reading_preferences_do_not_change_group_preferences() {
        tokio::task::spawn_blocking(|| {
            let preferences = HashMap::from([("sensor:cpu_usage".to_string(), false)]);
            let mut collector =
                SensorCollector::new(&preferences, &HashMap::new(), &HashMap::new());
            assert!(collector.is_enabled("cpu_usage"));
            assert!(!collector.is_enabled("sensor:cpu_usage"));
            let readings = collector.collect_dynamic();
            assert!(!readings
                .iter()
                .any(|sensor| sensor.unique_id == "cpu_usage"));
            let choices = collector.get_sensor_list();
            assert!(
                !choices
                    .iter()
                    .find(|choice| choice.id == "sensor:cpu_usage")
                    .unwrap()
                    .enabled
            );
        })
        .await
        .unwrap();
        assert_eq!(sensor_group("gpu_temperature_stable_1"), Some("gpu"));
        assert_eq!(sensor_group("display_resolution"), Some("display"));
    }

    #[test]
    fn disk_capacity_attributes_are_decimal_gb_numbers() {
        let partition = disk::PartitionData {
            name: "example".into(),
            mount_point: "/".into(),
            physical_id: None,
            total_bytes: 1_000_000_000,
            used_bytes: 500_000_000,
            available_bytes: 500_000_000,
            usage_percent: 50.0,
            filesystem: "ext4".into(),
            disk_type: "SSD".into(),
        };
        let attributes = disk_attributes(&partition);
        assert_eq!(attributes["total_gb"], serde_json::json!(1.0));
        assert_eq!(attributes["used_gb"], serde_json::json!(0.5));
        assert!(attributes["total_gb"].is_number());
    }

    #[test]
    fn verified_gpu_aliases_restore_legacy_ids_without_reassigning_another_gpu() {
        let mut sensors = vec![SensorValue {
            unique_id: "gpu_usage_stable_1".into(),
            name: "GPU Usage 0".into(),
            state: serde_json::json!(31.5),
            sensor_type: "sensor".into(),
            device_class: None,
            unit_of_measurement: Some("%".into()),
            state_class: Some("measurement".into()),
            icon: None,
            attributes: HashMap::new(),
            update_at_interval: true,
        }];
        let aliases = HashMap::from([(
            "gpu:nvml:verified-card".into(),
            vec![String::new(), "_0".into(), "_stable_2".into()],
        )]);
        let reserved = HashSet::from(["_stable_1".into(), "_stable_2".into()]);
        add_legacy_gpu_aliases(
            &mut sensors,
            0,
            "_stable_1",
            Some("nvml:verified-card"),
            &aliases,
            &reserved,
        );
        assert_eq!(
            sensors
                .iter()
                .map(|sensor| sensor.unique_id.as_str())
                .collect::<Vec<_>>(),
            ["gpu_usage_stable_1", "gpu_usage", "gpu_usage_0"]
        );
        assert!(sensors.iter().all(|sensor| sensor.state == 31.5));
    }

    #[test]
    fn physical_sensor_ids_survive_hotplug_and_reordering() {
        let mut map = HashMap::new();
        let first = assign_sensor_suffixes(&mut map, "gpu", &[Some("pci:a".into())], true);
        assert_eq!(first, vec![Some(String::new())]);
        let added = assign_sensor_suffixes(
            &mut map,
            "gpu",
            &[Some("pci:a".into()), Some("pci:b".into())],
            true,
        );
        assert_eq!(added, vec![Some(String::new()), Some("_stable_1".into())]);
        let reordered = assign_sensor_suffixes(
            &mut map,
            "gpu",
            &[Some("pci:b".into()), Some("pci:a".into())],
            true,
        );
        assert_eq!(
            reordered,
            vec![Some("_stable_1".into()), Some(String::new())]
        );
        let restored = assign_sensor_suffixes(&mut map, "gpu", &[Some("pci:b".into())], true);
        assert_eq!(restored, vec![Some("_stable_1".into())]);
    }

    #[test]
    fn network_and_disk_ids_never_claim_an_unverified_legacy_name() {
        let mut map = HashMap::new();
        let first = assign_sensor_suffixes(&mut map, "network", &[Some("mac:first".into())], false);
        assert_eq!(first, vec![Some("_stable_1".into())]);
        let replacement = assign_sensor_suffixes(
            &mut map,
            "network",
            &[Some("mac:replacement".into())],
            false,
        );
        assert_eq!(replacement, vec![Some("_stable_2".into())]);
        let restored =
            assign_sensor_suffixes(&mut map, "network", &[Some("mac:first".into())], false);
        assert_eq!(restored, first);
        assert_eq!(
            assign_sensor_suffixes(&mut map, "disk", &[None], false),
            vec![None]
        );
    }

    #[test]
    fn verified_legacy_suffix_survives_reordering_and_identity_collisions() {
        let mut map = HashMap::from([
            ("network:mac:aa:bb:cc:00:11:22".into(), "_Ethernet".into()),
            ("disk:volume:known".into(), "_C".into()),
        ]);
        assert_eq!(
            assign_sensor_suffixes(
                &mut map,
                "network",
                &[Some("mac:aa:bb:cc:00:11:22".into())],
                false
            ),
            vec![Some("_Ethernet".into())]
        );
        assert_eq!(
            assign_sensor_suffixes(
                &mut map,
                "disk",
                &[Some("volume:known".into()), Some("volume:new".into())],
                false
            ),
            vec![Some("_C".into()), Some("_stable_1".into())]
        );
        assert_eq!(
            assign_sensor_suffixes(
                &mut map,
                "network",
                &[
                    Some("mac:aa:bb:cc:00:11:22".into()),
                    Some("mac:aa:bb:cc:00:11:22".into()),
                ],
                false
            ),
            vec![None, None]
        );
    }

    #[test]
    fn ambiguous_hardware_never_reuses_an_index_id() {
        let mut map = HashMap::new();
        let first_multi = assign_sensor_suffixes(
            &mut map,
            "gpu",
            &[Some("pci:a".into()), Some("pci:b".into())],
            true,
        );
        assert_eq!(
            first_multi,
            vec![Some("_stable_1".into()), Some("_stable_2".into())]
        );
        assert_eq!(
            assign_sensor_suffixes(
                &mut map,
                "gpu",
                &[Some("pci:b".into()), Some("pci:b".into())],
                true,
            ),
            vec![None, None]
        );
        assert_eq!(
            assign_sensor_suffixes(&mut map, "gpu", &[None], true),
            vec![None]
        );
    }

    #[test]
    fn format_boot_time_returns_iso_for_known_epoch() {
        // 2026-05-26 12:00:00 UTC = 1779796800
        assert_eq!(
            format_boot_time(1779796800),
            Some("2026-05-26T12:00:00+00:00".to_string())
        );
    }

    #[test]
    fn format_boot_time_returns_none_for_zero() {
        assert_eq!(format_boot_time(0), None);
    }

    #[test]
    fn format_boot_time_handles_unix_epoch_plus_one() {
        // Smallest non-zero, ensures we don't accidentally return None for tiny values.
        assert_eq!(
            format_boot_time(1),
            Some("1970-01-01T00:00:01+00:00".to_string())
        );
    }

    #[test]
    fn build_uptime_sensor_state_is_numeric_seconds() {
        // 17381 = 4h 49m
        let sensor = build_uptime_sensor(17381);

        assert_eq!(sensor.state, serde_json::json!(17381));
        assert_eq!(sensor.attributes["human_readable"], "4h 49m");
    }

    #[test]
    fn build_uptime_sensor_state_with_days() {
        // 1d 1h 1m
        let sensor = build_uptime_sensor(90061);
        assert_eq!(sensor.state, serde_json::json!(90061));
        assert_eq!(sensor.attributes["human_readable"], "1d 1h 1m");
    }

    #[test]
    fn build_uptime_sensor_state_minutes_only() {
        // 49m
        let sensor = build_uptime_sensor(2940);
        assert_eq!(sensor.state, serde_json::json!(2940));
        assert_eq!(sensor.attributes["human_readable"], "0h 49m");
    }

    #[test]
    fn build_uptime_sensor_declares_numeric_contract() {
        let sensor = build_uptime_sensor(3725);

        assert_eq!(sensor.unique_id, "system_uptime");
        assert_eq!(sensor.sensor_type, "sensor");
        assert_eq!(sensor.device_class.as_deref(), Some("duration"));
        assert_eq!(sensor.state_class.as_deref(), Some("measurement"));
        assert_eq!(sensor.unit_of_measurement.as_deref(), Some("s"));
        assert!(sensor.update_at_interval);
    }

    #[test]
    fn build_uptime_sensor_keeps_numeric_data_in_attributes() {
        let sensor = build_uptime_sensor(90061); // 1d 1h 1m 1s

        // Power users can still graph or use these in automations via attributes.
        assert_eq!(
            sensor.attributes.get("uptime_seconds"),
            Some(&serde_json::json!(90061))
        );
        assert_eq!(sensor.attributes.get("days"), Some(&serde_json::json!(1)));
        assert_eq!(sensor.attributes.get("hours"), Some(&serde_json::json!(25)));
        assert_eq!(
            sensor.attributes.get("minutes"),
            Some(&serde_json::json!(1))
        );
    }

    #[test]
    fn build_last_boot_sensor_returns_none_for_zero() {
        assert!(build_last_boot_sensor(0).is_none());
    }

    #[test]
    fn build_last_boot_sensor_emits_timestamp_contract() {
        let sensor = build_last_boot_sensor(1779796800).expect("must be Some");

        assert_eq!(sensor.unique_id, "last_boot");
        assert_eq!(sensor.device_class.as_deref(), Some("timestamp"));
        assert_eq!(sensor.state, serde_json::json!("2026-05-26T12:00:00+00:00"),);
        // Power users keep the ISO + epoch in attributes.
        assert_eq!(
            sensor.attributes.get("boot_timestamp"),
            Some(&serde_json::json!(1779796800u64)),
        );
        assert_eq!(
            sensor.attributes.get("iso_utc"),
            Some(&serde_json::json!("2026-05-26T12:00:00+00:00")),
        );
        assert!(!sensor.update_at_interval, "last_boot is static");
    }
}
