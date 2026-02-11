use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryData {
    pub batteries: Vec<BatteryInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub percentage: f32,
    pub state: String,
    pub state_of_health: Option<f32>,
    pub cycle_count: Option<u32>,
    pub is_charging: bool,
}

pub fn collect() -> BatteryData {
    let batteries = collect_batteries();
    BatteryData { batteries }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn collect_batteries() -> Vec<BatteryInfo> {
    let manager = match battery::Manager::new() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    let mut batteries = Vec::new();
    if let Ok(battery_iter) = manager.batteries() {
        for battery_result in battery_iter {
            if let Ok(battery) = battery_result {
                let state = match battery.state() {
                    battery::State::Charging => "Charging",
                    battery::State::Discharging => "Discharging",
                    battery::State::Full => "Full",
                    battery::State::Empty => "Empty",
                    _ => "Unknown",
                };

                let is_charging = matches!(battery.state(), battery::State::Charging);

                batteries.push(BatteryInfo {
                    percentage: battery.state_of_charge().value * 100.0,
                    state: state.to_string(),
                    state_of_health: Some(battery.state_of_health().value * 100.0),
                    cycle_count: battery.cycle_count(),
                    is_charging,
                });
            }
        }
    }

    batteries
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn collect_batteries() -> Vec<BatteryInfo> {
    Vec::new()
}
