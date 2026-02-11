use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryData {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f32,
    pub total_gb: f64,
    pub used_gb: f64,
    pub available_gb: f64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

pub fn collect(sys: &System) -> MemoryData {
    let total = sys.total_memory();
    let used = sys.used_memory();
    let available = sys.available_memory();
    let usage_percent = if total > 0 {
        (used as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    MemoryData {
        total_bytes: total,
        used_bytes: used,
        available_bytes: available,
        usage_percent,
        total_gb: total as f64 / 1_073_741_824.0,
        used_gb: used as f64 / 1_073_741_824.0,
        available_gb: available as f64 / 1_073_741_824.0,
        swap_total_bytes: sys.total_swap(),
        swap_used_bytes: sys.used_swap(),
    }
}
