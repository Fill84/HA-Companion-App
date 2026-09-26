//! Read-only Linux hwmon channels. Discovery is separate from sampling so a
//! normal update reads known channel files without walking the device tree.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Temperature,
    Fan,
    Voltage,
    Current,
    Power,
}

impl ChannelKind {
    fn from_file_name(name: &str) -> Option<(Self, &str)> {
        let stem = name.strip_suffix("_input")?;
        for (prefix, kind) in [
            ("temp", Self::Temperature),
            ("fan", Self::Fan),
            ("in", Self::Voltage),
            ("curr", Self::Current),
            ("power", Self::Power),
        ] {
            if let Some(number) = stem.strip_prefix(prefix) {
                if !number.is_empty() && number.chars().all(|digit| digit.is_ascii_digit()) {
                    return Some((kind, stem));
                }
            }
        }
        None
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Temperature => "°C",
            Self::Fan => "rpm",
            Self::Voltage => "V",
            Self::Current => "A",
            Self::Power => "W",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Temperature => "Temperature",
            Self::Fan => "Fan",
            Self::Voltage => "Voltage",
            Self::Current => "Current",
            Self::Power => "Power",
        }
    }

    fn convert(self, raw: i64) -> Option<f64> {
        let value = match self {
            Self::Temperature if (-40_000..=150_000).contains(&raw) => raw as f64 / 1_000.0,
            Self::Fan if (0..=100_000).contains(&raw) => raw as f64,
            Self::Voltage if (0..=1_000_000).contains(&raw) => raw as f64 / 1_000.0,
            Self::Current if (0..=1_000_000_000).contains(&raw) => raw as f64 / 1_000.0,
            Self::Power if (0..=1_000_000_000_000).contains(&raw) => raw as f64 / 1_000_000.0,
            _ => return None,
        };
        value.is_finite().then_some(value)
    }
}

#[derive(Debug)]
struct Channel {
    path: PathBuf,
    fault_path: Option<PathBuf>,
    enable_path: Option<PathBuf>,
    unique_id: String,
    name: String,
    chip: String,
    channel: String,
    kind: ChannelKind,
}

#[derive(Debug)]
pub struct Reading {
    pub unique_id: String,
    pub name: String,
    pub chip: String,
    pub channel: String,
    pub kind: ChannelKind,
    pub value: Option<f64>,
}

pub struct HwmonCollector {
    root: PathBuf,
    channels: Vec<Channel>,
    discovered: bool,
}

impl Default for HwmonCollector {
    fn default() -> Self {
        Self::new(PathBuf::from("/sys/class/hwmon"))
    }
}

impl HwmonCollector {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            channels: Vec::new(),
            discovered: false,
        }
    }

    pub fn collect(&mut self, refresh_topology: bool) -> Vec<Reading> {
        if refresh_topology || !self.discovered {
            self.channels = discover_from(&self.root);
            self.discovered = true;
        }
        let mut missing = false;
        let readings = self
            .channels
            .iter()
            .map(|channel| {
                let raw = fs::read_to_string(&channel.path);
                if raw
                    .as_ref()
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                {
                    missing = true;
                }
                let value = raw
                    .ok()
                    .and_then(|text| text.trim().parse::<i64>().ok())
                    .and_then(|raw| channel.kind.convert(raw))
                    .filter(|_| channel_is_enabled(channel));
                Reading {
                    unique_id: channel.unique_id.clone(),
                    name: channel.name.clone(),
                    chip: channel.chip.clone(),
                    channel: channel.channel.clone(),
                    kind: channel.kind,
                    value,
                }
            })
            .collect();
        if missing {
            self.discovered = false;
        }
        readings
    }
}

fn discover_from(root: &Path) -> Vec<Channel> {
    let mut channels = Vec::new();
    let Ok(chips) = fs::read_dir(root) else {
        return channels;
    };
    for chip in chips.flatten() {
        let path = chip.path();
        let Some(device_path) = stable_device_path(&path) else {
            continue;
        };
        let Some(chip_name) = fs::read_to_string(path.join("name"))
            .ok()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty() && name.len() <= 80)
        else {
            continue;
        };
        let Ok(files) = fs::read_dir(&path) else {
            continue;
        };
        for file in files.flatten() {
            let Some(file_name) = file.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some((kind, stem)) = ChannelKind::from_file_name(&file_name) else {
                continue;
            };
            let label = fs::read_to_string(path.join(format!("{stem}_label")))
                .ok()
                .map(|label| label.trim().to_string())
                .filter(|label| !label.is_empty() && label.len() <= 80)
                .unwrap_or_else(|| format!("{} {}", kind.label(), stem));
            let identity = format!("{}|{}|{}", device_path.display(), chip_name, stem);
            let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, identity.as_bytes());
            channels.push(Channel {
                path: file.path(),
                fault_path: path
                    .join(format!("{stem}_fault"))
                    .exists()
                    .then(|| path.join(format!("{stem}_fault"))),
                enable_path: path
                    .join(format!("{stem}_enable"))
                    .exists()
                    .then(|| path.join(format!("{stem}_enable"))),
                unique_id: format!("hardware_{}", id.simple()),
                name: format!("{} {}", chip_name, label),
                chip: chip_name.clone(),
                channel: stem.to_string(),
                kind,
            });
        }
    }
    channels.sort_by(|a, b| a.unique_id.cmp(&b.unique_id));
    channels.dedup_by(|a, b| a.unique_id == b.unique_id);
    channels
}

fn channel_is_enabled(channel: &Channel) -> bool {
    let read_flag = |path: &Path| {
        fs::read_to_string(path)
            .ok()
            .and_then(|value| value.trim().parse::<u8>().ok())
    };
    channel
        .fault_path
        .as_deref()
        .is_none_or(|path| read_flag(path) == Some(0))
        && channel
            .enable_path
            .as_deref()
            .is_none_or(|path| read_flag(path) == Some(1))
}

fn stable_device_path(path: &Path) -> Option<PathBuf> {
    let canonical = fs::canonicalize(path).ok()?;
    if let Ok(device) = fs::canonicalize(path.join("device")) {
        return Some(device);
    }
    if canonical.parent()?.file_name()? == "hwmon" {
        return Some(canonical.parent()?.parent()?.to_path_buf());
    }
    Some(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_known_units_and_keeps_ids_across_samples() {
        let root = std::env::temp_dir().join(format!(
            "ha-hwmon-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let chip = root.join("hwmon0");
        fs::create_dir_all(chip.join("device")).unwrap();
        fs::write(chip.join("name"), "testchip\n").unwrap();
        fs::write(chip.join("temp1_input"), "52000\n").unwrap();
        fs::write(chip.join("temp1_label"), "CPU Die\n").unwrap();
        fs::write(chip.join("fan1_input"), "1200\n").unwrap();
        fs::write(chip.join("in1_input"), "1200\n").unwrap();
        fs::write(chip.join("power1_input"), "35000000\n").unwrap();
        fs::write(chip.join("fan1_fault"), "0\n").unwrap();
        let mut collector = HwmonCollector::new(root.clone());
        let first = collector.collect(false);
        assert_eq!(first.len(), 4);
        assert!(first
            .iter()
            .any(|reading| reading.name == "testchip CPU Die" && reading.value == Some(52.0)));
        assert!(first
            .iter()
            .any(|reading| reading.kind == ChannelKind::Fan && reading.value == Some(1200.0)));
        assert!(first
            .iter()
            .any(|reading| reading.kind == ChannelKind::Voltage && reading.value == Some(1.2)));
        assert!(first
            .iter()
            .any(|reading| reading.kind == ChannelKind::Power && reading.value == Some(35.0)));
        assert!(first.iter().all(|reading| reading.chip == "testchip"));
        assert!(first.iter().any(|reading| reading.channel == "temp1"));
        assert_eq!(ChannelKind::Power.unit(), "W");
        fs::write(chip.join("temp1_input"), "999999\n").unwrap();
        fs::write(chip.join("fan1_fault"), "1\n").unwrap();
        let second = collector.collect(false);
        assert!(second
            .iter()
            .any(|reading| reading.kind == ChannelKind::Temperature && reading.value.is_none()));
        assert!(second
            .iter()
            .any(|reading| reading.kind == ChannelKind::Fan && reading.value.is_none()));
        assert_eq!(
            first
                .iter()
                .map(|reading| &reading.unique_id)
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|reading| &reading.unique_id)
                .collect::<Vec<_>>()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
