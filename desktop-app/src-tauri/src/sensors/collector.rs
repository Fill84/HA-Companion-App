use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sysinfo::System;

use super::{battery, cpu, disk, gpu, memory, network, system_info};

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
}

impl SensorCollector {
    pub fn new(enabled_sensors: &HashMap<String, bool>) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        Self {
            sys,
            enabled_sensors: enabled_sensors.clone(),
        }
    }

    fn is_enabled(&self, sensor_id: &str) -> bool {
        *self.enabled_sensors.get(sensor_id).unwrap_or(&true)
    }

    /// Collect all sensors (both static and dynamic) — used at startup
    pub fn collect_all(&mut self) -> Vec<SensorValue> {
        self.sys.refresh_all();
        let mut sensors = Vec::new();

        sensors.extend(self.collect_static());
        sensors.extend(self.collect_dynamic());

        sensors
    }

    /// Collect only dynamic sensors — used at interval
    pub fn collect_dynamic(&mut self) -> Vec<SensorValue> {
        self.sys.refresh_all();
        let mut sensors = Vec::new();

        // CPU sensors (dynamic)
        if self.is_enabled("cpu_usage") {
            let cpu_data = cpu::collect(&self.sys);
            sensors.push(SensorValue {
                unique_id: "cpu_usage".into(),
                name: "CPU Usage".into(),
                state: serde_json::json!(format!("{:.1}", cpu_data.usage_percent)),
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
            let cpu_data = cpu::collect(&self.sys);
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
            let cpu_data = cpu::collect(&self.sys);
            if let Some(temp) = cpu_data.temperature {
                sensors.push(SensorValue {
                    unique_id: "cpu_temperature".into(),
                    name: "CPU Temperature".into(),
                    state: serde_json::json!(format!("{:.1}", temp)),
                    sensor_type: "sensor".into(),
                    device_class: Some("temperature".into()),
                    unit_of_measurement: Some("°C".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:thermometer".into()),
                    attributes: HashMap::new(),
                    update_at_interval: true,
                });
            }
        }

        // Memory sensors (dynamic)
        if self.is_enabled("memory_usage") {
            let mem_data = memory::collect(&self.sys);
            sensors.push(SensorValue {
                unique_id: "memory_usage".into(),
                name: "Memory Usage".into(),
                state: serde_json::json!(format!("{:.1}", mem_data.usage_percent)),
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
            let mem_data = memory::collect(&self.sys);
            sensors.push(SensorValue {
                unique_id: "memory_used".into(),
                name: "Memory Used".into(),
                state: serde_json::json!(format!("{:.2}", mem_data.used_gb)),
                sensor_type: "sensor".into(),
                device_class: Some("data_size".into()),
                unit_of_measurement: Some("GB".into()),
                state_class: Some("measurement".into()),
                icon: Some("mdi:memory".into()),
                attributes: HashMap::new(),
                update_at_interval: true,
            });
        }

        // Disk sensors (dynamic)
        if self.is_enabled("disk_usage") {
            let disk_data = disk::collect();
            for partition in &disk_data.partitions {
                let safe_name = partition
                    .mount_point
                    .replace(['/', '\\', ':'], "_")
                    .trim_matches('_')
                    .to_string();

                sensors.push(SensorValue {
                    unique_id: format!("disk_usage_{}", safe_name),
                    name: format!("Disk Usage {}", partition.mount_point),
                    state: serde_json::json!(format!("{:.1}", partition.usage_percent)),
                    sensor_type: "sensor".into(),
                    device_class: None,
                    unit_of_measurement: Some("%".into()),
                    state_class: Some("measurement".into()),
                    icon: Some("mdi:harddisk".into()),
                    attributes: {
                        let mut attrs = HashMap::new();
                        attrs.insert(
                            "total_gb".into(),
                            serde_json::json!(
                                format!("{:.1}", partition.total_bytes as f64 / 1_073_741_824.0)
                            ),
                        );
                        attrs.insert(
                            "used_gb".into(),
                            serde_json::json!(
                                format!("{:.1}", partition.used_bytes as f64 / 1_073_741_824.0)
                            ),
                        );
                        attrs.insert("filesystem".into(), serde_json::json!(partition.filesystem));
                        attrs.insert("disk_type".into(), serde_json::json!(partition.disk_type));
                        attrs
                    },
                    update_at_interval: true,
                });
            }
        }

        // GPU sensors (dynamic)
        if self.is_enabled("gpu") {
            let gpu_data = gpu::collect();
            for (i, gpu_info) in gpu_data.gpus.iter().enumerate() {
                let suffix = if gpu_data.gpus.len() > 1 {
                    format!("_{}", i)
                } else {
                    String::new()
                };

                if let Some(usage) = gpu_info.usage_percent {
                    sensors.push(SensorValue {
                        unique_id: format!("gpu_usage{}", suffix),
                        name: format!("GPU Usage{}", if suffix.is_empty() { "".to_string() } else { format!(" {}", i) }),
                        state: serde_json::json!(format!("{:.1}", usage)),
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
                        name: format!("GPU Temperature{}", if suffix.is_empty() { "".to_string() } else { format!(" {}", i) }),
                        state: serde_json::json!(format!("{:.1}", temp)),
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
                        name: format!("GPU VRAM Used{}", if suffix.is_empty() { "".to_string() } else { format!(" {}", i) }),
                        state: serde_json::json!(format!("{:.0}", vram_used)),
                        sensor_type: "sensor".into(),
                        device_class: Some("data_size".into()),
                        unit_of_measurement: Some("MB".into()),
                        state_class: Some("measurement".into()),
                        icon: Some("mdi:expansion-card-variant".into()),
                        attributes: HashMap::new(),
                        update_at_interval: true,
                    });
                }
            }
        }

        // Network sensors (dynamic)
        if self.is_enabled("network") {
            let net_data = network::collect();
            for iface in &net_data.interfaces {
                let safe_name = iface.name.replace([' ', '/', '\\'], "_");
                sensors.push(SensorValue {
                    unique_id: format!("network_rx_{}", safe_name),
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
                        attrs.insert(
                            "ip_addresses".into(),
                            serde_json::json!(iface.ip_addresses),
                        );
                        attrs
                    },
                    update_at_interval: true,
                });

                sensors.push(SensorValue {
                    unique_id: format!("network_tx_{}", safe_name),
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
            for (i, bat) in battery_data.batteries.iter().enumerate() {
                let suffix = if battery_data.batteries.len() > 1 {
                    format!("_{}", i)
                } else {
                    String::new()
                };

                sensors.push(SensorValue {
                    unique_id: format!("battery_level{}", suffix),
                    name: format!("Battery Level{}", if suffix.is_empty() { "".to_string() } else { format!(" {}", i) }),
                    state: serde_json::json!(format!("{:.0}", bat.percentage)),
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
                    name: format!("Battery Charging{}", if suffix.is_empty() { "".to_string() } else { format!(" {}", i) }),
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

        if self.is_enabled("bios_version") {
            if let Some(ref bios) = sys_info.bios_version {
                sensors.push(SensorValue {
                    unique_id: "bios_version".into(),
                    name: "BIOS Version".into(),
                    state: serde_json::json!(bios),
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

        // GPU model (static)
        if self.is_enabled("gpu") {
            let gpu_data = gpu::collect();
            for (i, gpu_info) in gpu_data.gpus.iter().enumerate() {
                let suffix = if gpu_data.gpus.len() > 1 {
                    format!("_{}", i)
                } else {
                    String::new()
                };

                sensors.push(SensorValue {
                    unique_id: format!("gpu_model{}", suffix),
                    name: format!("GPU Model{}", if suffix.is_empty() { "".to_string() } else { format!(" {}", i) }),
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
            }
        }

        // RAM total (static)
        if self.is_enabled("memory_total") {
            let mem_data = memory::collect(&self.sys);
            sensors.push(SensorValue {
                unique_id: "memory_total".into(),
                name: "Memory Total".into(),
                state: serde_json::json!(format!("{:.1}", mem_data.total_gb)),
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
    pub fn get_sensor_list(&self) -> Vec<SensorListItem> {
        let all_sensors = vec![
            ("cpu_usage", "CPU Usage", true),
            ("cpu_frequency", "CPU Frequency", true),
            ("cpu_temperature", "CPU Temperature", true),
            ("cpu_model", "CPU Model", false),
            ("memory_usage", "Memory Usage", true),
            ("memory_used", "Memory Used", true),
            ("memory_total", "Memory Total", false),
            ("disk_usage", "Disk Usage", true),
            ("gpu", "GPU Sensors", true),
            ("network", "Network Sensors", true),
            ("battery", "Battery Sensors", true),
            ("os_version", "OS Version", false),
            ("hostname", "Hostname", false),
            ("motherboard", "Motherboard", false),
            ("bios_version", "BIOS Version", false),
        ];

        all_sensors
            .into_iter()
            .map(|(id, name, updates_at_interval)| SensorListItem {
                id: id.to_string(),
                name: name.to_string(),
                enabled: self.is_enabled(id),
                updates_at_interval,
            })
            .collect()
    }

    /// Update enabled sensors map
    pub fn set_enabled_sensors(&mut self, enabled: HashMap<String, bool>) {
        self.enabled_sensors = enabled;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorListItem {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub updates_at_interval: bool,
}
