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

    // Try to get CPU temperature from components
    let temperature = {
        let components = sysinfo::Components::new_with_refreshed_list();
        components
            .iter()
            .find(|c| {
                let label = c.label().to_lowercase();
                label.contains("cpu") || label.contains("core") || label.contains("package")
            })
            .map(|c| c.temperature())
    };

    CpuData {
        model,
        usage_percent,
        frequency_mhz,
        temperature,
        core_count,
        logical_core_count,
    }
}
