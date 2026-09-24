use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuData {
    pub model: String,
    pub usage_percent: f32,
    pub frequency_mhz: u64,
    pub core_count: usize,
    pub logical_core_count: usize,
}

/// Basic CPU data only. Temperature has its own explicitly enabled provider.
pub fn collect(sys: &System) -> CpuData {
    let cpus = sys.cpus();
    CpuData {
        model: cpus
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default(),
        usage_percent: sys.global_cpu_usage(),
        frequency_mhz: cpus.first().map(|c| c.frequency()).unwrap_or(0),
        core_count: sys.physical_core_count().unwrap_or(0),
        logical_core_count: cpus.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_system_does_not_trigger_discovery() {
        let data = collect(&System::new());
        assert_eq!(data.logical_core_count, 0);
        assert_eq!(data.frequency_mhz, 0);
        assert!(data.model.is_empty());
    }
}
