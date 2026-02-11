use serde::{Deserialize, Serialize};
use sysinfo::Disks;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskData {
    pub partitions: Vec<PartitionData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionData {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f32,
    pub filesystem: String,
    pub disk_type: String,
}

pub fn collect() -> DiskData {
    let disks = Disks::new_with_refreshed_list();
    let partitions: Vec<PartitionData> = disks
        .iter()
        .map(|disk| {
            let total = disk.total_space();
            let available = disk.available_space();
            let used = total.saturating_sub(available);
            let usage_percent = if total > 0 {
                (used as f32 / total as f32) * 100.0
            } else {
                0.0
            };

            let disk_type = match disk.kind() {
                sysinfo::DiskKind::SSD => "SSD".to_string(),
                sysinfo::DiskKind::HDD => "HDD".to_string(),
                _ => "Unknown".to_string(),
            };

            PartitionData {
                name: disk.name().to_string_lossy().to_string(),
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                total_bytes: total,
                used_bytes: used,
                available_bytes: available,
                usage_percent,
                filesystem: disk.file_system().to_string_lossy().to_string(),
                disk_type,
            }
        })
        .collect();

    DiskData { partitions }
}
