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
    pub physical_id: Option<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f32,
    pub filesystem: String,
    pub disk_type: String,
}

#[cfg(windows)]
fn volume_identity(mount_point: &std::path::Path, _name: &std::ffi::OsStr) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetVolumeNameForVolumeMountPointW(
            mount_point: *const u16,
            volume_name: *mut u16,
            buffer_length: u32,
        ) -> i32;
    }
    let path: Vec<u16> = mount_point
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut volume = [0u16; 64];
    if unsafe {
        GetVolumeNameForVolumeMountPointW(path.as_ptr(), volume.as_mut_ptr(), volume.len() as u32)
    } == 0
    {
        return None;
    }
    let length = volume.iter().position(|character| *character == 0)?;
    let id = String::from_utf16(&volume[..length]).ok()?;
    id.to_ascii_lowercase()
        .contains("volume{")
        .then(|| format!("volume:{}", id.to_ascii_lowercase()))
}

#[cfg(target_os = "linux")]
fn volume_identity(_mount_point: &std::path::Path, name: &std::ffi::OsStr) -> Option<String> {
    let device = std::fs::canonicalize(std::path::Path::new(name)).ok()?;
    for entry in std::fs::read_dir("/dev/disk/by-uuid").ok()?.flatten() {
        if std::fs::canonicalize(entry.path()).ok().as_ref() == Some(&device) {
            let uuid = entry.file_name().to_string_lossy().to_string();
            if !uuid.is_empty() && uuid.len() <= 128 {
                return Some(format!("fsuuid:{}", uuid.to_ascii_lowercase()));
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn volume_identity(mount_point: &std::path::Path, _name: &std::ffi::OsStr) -> Option<String> {
    use std::ffi::{c_char, c_void, CStr};
    use std::os::unix::ffi::OsStrExt;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFURLVolumeUUIDStringKey: *const c_void;
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: *const c_void,
            path: *const u8,
            length: isize,
            is_directory: u8,
        ) -> *const c_void;
        fn CFURLCopyResourcePropertyForKey(
            url: *const c_void,
            key: *const c_void,
            value: *mut *const c_void,
            error: *mut *const c_void,
        ) -> u8;
        fn CFStringGetCString(
            value: *const c_void,
            buffer: *mut c_char,
            length: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }

    let path = mount_point.as_os_str().as_bytes();
    let url = unsafe {
        CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            path.as_ptr(),
            path.len() as isize,
            1,
        )
    };
    if url.is_null() {
        return None;
    }
    let mut value = std::ptr::null();
    let mut error = std::ptr::null();
    let success = unsafe {
        CFURLCopyResourcePropertyForKey(url, kCFURLVolumeUUIDStringKey, &mut value, &mut error)
    } != 0;
    let result = if success && !value.is_null() {
        let mut buffer = [0 as c_char; 128];
        let copied = unsafe {
            CFStringGetCString(
                value,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                0x0800_0100,
            )
        } != 0;
        copied
            .then(|| unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_str().ok())
            .flatten()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .map(|id| format!("volume:{id}"))
    } else {
        None
    };
    unsafe {
        if !value.is_null() {
            CFRelease(value);
        }
        if !error.is_null() {
            CFRelease(error);
        }
        CFRelease(url);
    }
    result
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn volume_identity(_mount_point: &std::path::Path, _name: &std::ffi::OsStr) -> Option<String> {
    None
}

pub struct DiskCollector {
    disks: Disks,
    discovered: bool,
}

impl Default for DiskCollector {
    fn default() -> Self {
        Self {
            disks: Disks::new(),
            discovered: false,
        }
    }
}

impl DiskCollector {
    pub fn collect(&mut self, refresh_topology: bool) -> DiskData {
        if refresh_topology || !self.discovered {
            self.disks.refresh_list();
            self.discovered = true;
        } else {
            self.disks.refresh();
        }
        self.snapshot()
    }

    fn snapshot(&self) -> DiskData {
        let partitions: Vec<PartitionData> = self
            .disks
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
                    physical_id: volume_identity(disk.mount_point(), disk.name()),
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
}

#[cfg(all(test, windows))]
fn collect() -> DiskData {
    DiskCollector::default().collect(true)
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    #[ignore = "requires a Windows host with a mounted volume"]
    fn mounted_windows_volumes_have_stable_guids() {
        let partitions = super::collect().partitions;
        assert!(partitions.iter().any(|disk| disk
            .physical_id
            .as_deref()
            .is_some_and(|id| id.starts_with("volume:"))));
    }
}
