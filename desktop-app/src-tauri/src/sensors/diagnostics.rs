use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde::Serialize;

use super::collector::SensorValue;

#[derive(Default)]
pub struct SensorDiagnostics {
    readings: HashMap<String, ReadingDiagnostic>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReadingDiagnostic {
    pub name: String,
    pub current_value: Option<String>,
    pub last_value: Option<String>,
    pub unit: Option<String>,
    pub source: Option<String>,
    pub last_read_at: i64,
    pub last_successful_read_at: Option<i64>,
    pub last_ha_ack_at: Option<i64>,
    pub reason: Option<String>,
    pub updates_at_interval: bool,
}

fn display_value(sensor: &SensorValue) -> Option<String> {
    match &sensor.state {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) if value.is_empty() => None,
        serde_json::Value::String(value) => Some(value.clone()),
        value => Some(value.to_string()),
    }
}

fn source(sensor: &SensorValue) -> Option<String> {
    if sensor.attributes.contains_key("measurement_source") {
        return sensor
            .attributes
            .get("measurement_source")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
    }
    let id = sensor.unique_id.as_str();
    if id == "cpu_temperature" {
        return None;
    }
    let provider = if id.starts_with("gpu_") {
        "GPU collector"
    } else if id.starts_with("battery_") {
        "battery API"
    } else if id.starts_with("network_")
        || id.starts_with("disk_")
        || id.starts_with("cpu_")
        || id.starts_with("memory_")
        || id == "swap_usage"
        || id == "process_count"
        || id == "system_uptime"
    {
        "sysinfo"
    } else {
        "system collector"
    };
    Some(provider.to_owned())
}

impl SensorDiagnostics {
    pub fn record_snapshot(
        &mut self,
        sensors: &[SensorValue],
        full: bool,
        preferences: &HashMap<String, bool>,
    ) {
        let now = Utc::now().timestamp_millis();
        let present: HashSet<&str> = sensors
            .iter()
            .map(|sensor| sensor.unique_id.as_str())
            .collect();
        for (id, diagnostic) in &mut self.readings {
            if (full || diagnostic.updates_at_interval) && !present.contains(id.as_str()) {
                diagnostic.current_value = None;
                let group_enabled = super::collector::sensor_group(id)
                    .is_none_or(|group| preferences.get(group).copied().unwrap_or(true));
                let reading_enabled = preferences
                    .get(&format!("sensor:{id}"))
                    .copied()
                    .unwrap_or(true);
                if group_enabled && reading_enabled {
                    diagnostic.reason = Some("not_emitted".into());
                    diagnostic.last_read_at = now;
                } else {
                    diagnostic.reason = Some("disabled".into());
                }
            }
        }
        for sensor in sensors {
            let value = display_value(sensor);
            let reason = if value.is_some() {
                None
            } else {
                Some(
                    sensor
                        .attributes
                        .get("provider_status")
                        .and_then(|status| status.as_str())
                        .unwrap_or("no_value")
                        .to_owned(),
                )
            };
            let entry = self
                .readings
                .entry(sensor.unique_id.clone())
                .or_insert_with(|| ReadingDiagnostic {
                    name: sensor.name.clone(),
                    current_value: None,
                    last_value: None,
                    unit: sensor.unit_of_measurement.clone(),
                    source: None,
                    last_read_at: now,
                    last_successful_read_at: None,
                    last_ha_ack_at: None,
                    reason: None,
                    updates_at_interval: sensor.update_at_interval,
                });
            entry.current_value = value.clone();
            entry.name = sensor.name.clone();
            entry.unit = sensor.unit_of_measurement.clone();
            if let Some(source) = source(sensor) {
                entry.source = Some(source);
            }
            entry.reason = reason;
            entry.last_read_at = now;
            entry.updates_at_interval = sensor.update_at_interval;
            if let Some(value) = value {
                entry.last_value = Some(value);
                entry.last_successful_read_at = Some(now);
            }
        }
    }

    pub fn record_ha_ack(&mut self, sensors: &[SensorValue], full: bool) {
        let now = Utc::now().timestamp_millis();
        let present: HashSet<&str> = sensors
            .iter()
            .map(|sensor| sensor.unique_id.as_str())
            .collect();
        for (id, reading) in &mut self.readings {
            if (full || reading.updates_at_interval)
                && !present.contains(id.as_str())
                && matches!(reading.reason.as_deref(), Some("not_emitted" | "disabled"))
            {
                reading.last_ha_ack_at = Some(now);
            }
        }
        for sensor in sensors {
            if let Some(reading) = self.readings.get_mut(&sensor.unique_id) {
                if reading.current_value == display_value(sensor) {
                    reading.last_ha_ack_at = Some(now);
                }
            }
        }
    }

    pub fn snapshot(&self) -> HashMap<String, ReadingDiagnostic> {
        self.readings.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor(id: &str, state: serde_json::Value) -> SensorValue {
        SensorValue {
            unique_id: id.into(),
            name: id.into(),
            state,
            sensor_type: "sensor".into(),
            device_class: None,
            unit_of_measurement: Some("°C".into()),
            state_class: None,
            icon: None,
            attributes: HashMap::new(),
            update_at_interval: true,
        }
    }

    #[test]
    fn failed_measurement_keeps_last_good_value_but_marks_current_unknown() {
        let mut diagnostics = SensorDiagnostics::default();
        let mut good = sensor("cpu_temperature", serde_json::json!(52));
        good.attributes.insert(
            "measurement_source".into(),
            serde_json::json!("native/sysinfo"),
        );
        diagnostics.record_snapshot(&[good], true, &HashMap::new());
        diagnostics.record_ha_ack(&[sensor("cpu_temperature", serde_json::json!(52))], true);
        let mut bad = sensor("cpu_temperature", serde_json::Value::Null);
        bad.attributes.insert(
            "provider_status".into(),
            serde_json::json!("provider_timeout"),
        );
        diagnostics.record_snapshot(&[bad], false, &HashMap::new());
        let entry = &diagnostics.snapshot()["cpu_temperature"];
        assert_eq!(entry.current_value, None);
        assert_eq!(entry.last_value.as_deref(), Some("52"));
        assert_eq!(entry.source.as_deref(), Some("native/sysinfo"));
        assert_eq!(entry.reason.as_deref(), Some("provider_timeout"));
        assert!(entry.last_ha_ack_at.is_some());
    }

    #[test]
    fn missing_dynamic_reading_is_not_shown_as_current() {
        let mut diagnostics = SensorDiagnostics::default();
        diagnostics.record_snapshot(
            &[sensor("gpu_temperature", serde_json::json!(45))],
            true,
            &HashMap::new(),
        );
        diagnostics.record_snapshot(&[], false, &HashMap::new());
        let entry = &diagnostics.snapshot()["gpu_temperature"];
        assert_eq!(entry.current_value, None);
        assert_eq!(entry.reason.as_deref(), Some("not_emitted"));
    }

    #[test]
    fn older_ack_cannot_confirm_a_newer_different_value() {
        let mut diagnostics = SensorDiagnostics::default();
        let old = sensor("cpu_usage", serde_json::json!(10));
        diagnostics.record_snapshot(std::slice::from_ref(&old), false, &HashMap::new());
        diagnostics.record_snapshot(
            &[sensor("cpu_usage", serde_json::json!(80))],
            false,
            &HashMap::new(),
        );
        diagnostics.record_ha_ack(&[old], false);
        assert!(diagnostics.snapshot()["cpu_usage"].last_ha_ack_at.is_none());
    }

    #[test]
    fn full_ack_confirms_a_missing_reading_was_cleared_in_home_assistant() {
        let mut diagnostics = SensorDiagnostics::default();
        diagnostics.record_snapshot(
            &[sensor("gpu_temperature", serde_json::json!(45))],
            true,
            &HashMap::new(),
        );
        diagnostics.record_snapshot(&[], true, &HashMap::new());
        diagnostics.record_ha_ack(&[], true);
        let entry = &diagnostics.snapshot()["gpu_temperature"];
        assert!(entry.last_ha_ack_at.is_some());
        assert_eq!(entry.current_value, None);
    }

    #[test]
    fn disabled_reading_is_not_reported_as_a_failed_hardware_read() {
        let mut diagnostics = SensorDiagnostics::default();
        diagnostics.record_snapshot(
            &[sensor("gpu_temperature", serde_json::json!(45))],
            true,
            &HashMap::new(),
        );
        let preferences = HashMap::from([("gpu".into(), false)]);
        diagnostics.record_snapshot(&[], true, &preferences);
        let entry = &diagnostics.snapshot()["gpu_temperature"];
        assert_eq!(entry.reason.as_deref(), Some("disabled"));
        assert_eq!(entry.current_value, None);
        assert!(entry.last_successful_read_at.is_some());
    }
}
