#[cfg(any(windows, test))]
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Temperature {
    pub value: Option<f32>,
    pub status: String,
    pub source: Option<String>,
    pub labels: Vec<String>,
}

impl Temperature {
    pub fn unavailable(status: &str) -> Self {
        Self {
            value: None,
            status: status.into(),
            source: None,
            labels: Vec::new(),
        }
    }

    pub fn attributes(&self) -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("provider_status".into(), serde_json::json!(self.status)),
            ("measurement_source".into(), serde_json::json!(self.source)),
            ("sensor_labels".into(), serde_json::json!(self.labels)),
            (
                "aggregation".into(),
                serde_json::json!("maximum_package_or_die"),
            ),
        ])
    }
}

#[cfg(any(windows, test))]
#[derive(Deserialize)]
struct Snapshot {
    protocol: u8,
    request_id: u64,
    provider: String,
    provider_version: String,
    status: String,
    readings: Vec<Reading>,
}

#[cfg(any(windows, test))]
#[derive(Deserialize)]
struct Reading {
    hardware_id: String,
    label: String,
    celsius: f32,
}

#[cfg(any(windows, test))]
fn parse_snapshot(bytes: &[u8], request_id: u64) -> Result<Temperature, &'static str> {
    let snapshot: Snapshot = serde_json::from_slice(bytes).map_err(|_| "invalid_response")?;
    if snapshot.protocol != 1
        || snapshot.request_id != request_id
        || snapshot.provider != "librehardwaremonitor"
        || snapshot.provider_version != "0.9.6"
        || snapshot.readings.len() > 256
    {
        return Err("incompatible_provider");
    }
    if snapshot.status != "ok" {
        return match snapshot.status.as_str() {
            "driver_missing" | "driver_unsupported" | "unsupported" | "provider_error" => {
                Ok(Temperature::unavailable(&snapshot.status))
            }
            _ => Err("invalid_response"),
        };
    }
    let mut packages: HashMap<String, (u8, Reading)> = HashMap::new();
    for reading in snapshot.readings {
        if !(reading.hardware_id.starts_with("/intelcpu/")
            || reading.hardware_id.starts_with("/amdcpu/"))
            || !reading.celsius.is_finite()
            || !(0.0..150.0).contains(&reading.celsius)
            || reading.celsius == 0.0
        {
            continue;
        }
        let rank = match reading.label.as_str() {
            "CPU Package" | "Core (Tdie)" => 0,
            "Core (Tctl/Tdie)" => 1,
            _ => continue,
        };
        let replace = packages
            .get(&reading.hardware_id)
            .is_none_or(|(old_rank, old)| {
                rank < *old_rank || (rank == *old_rank && reading.celsius > old.celsius)
            });
        if replace {
            packages.insert(reading.hardware_id.clone(), (rank, reading));
        }
    }
    let value = packages.values().map(|(_, r)| r.celsius).reduce(f32::max);
    let mut labels: Vec<_> = packages.into_values().map(|(_, r)| r.label).collect();
    labels.sort();
    labels.dedup();
    Ok(Temperature {
        value,
        status: if value.is_some() { "ok" } else { "unsupported" }.into(),
        source: Some("librehardwaremonitor/0.9.6/pawnio".into()),
        labels,
    })
}

pub struct TemperatureReader {
    enabled: bool,
    #[cfg(windows)]
    helper_path: Option<PathBuf>,
    #[cfg(windows)]
    session: Option<process::Session>,
    #[cfg(windows)]
    retry_after: Option<std::time::Instant>,
    #[cfg(windows)]
    last_error: String,
    #[cfg(not(windows))]
    components: sysinfo::Components,
    #[cfg(not(windows))]
    discovery_after: Option<std::time::Instant>,
}

impl TemperatureReader {
    pub fn new(helper_path: Option<PathBuf>, enabled: bool) -> Self {
        #[cfg(not(windows))]
        let _ = helper_path;
        Self {
            enabled,
            #[cfg(windows)]
            helper_path,
            #[cfg(windows)]
            session: None,
            #[cfg(windows)]
            retry_after: None,
            #[cfg(windows)]
            last_error: "provider_unavailable".into(),
            #[cfg(not(windows))]
            components: sysinfo::Components::new(),
            #[cfg(not(windows))]
            discovery_after: None,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        #[cfg(windows)]
        {
            self.session = None;
            self.retry_after = None;
        }
    }

    pub fn suspend(&mut self) {
        #[cfg(windows)]
        {
            self.session = None;
        }
    }

    #[cfg(windows)]
    pub fn read(&mut self) -> Temperature {
        if !self.enabled {
            return Temperature::unavailable("disabled");
        }
        if self
            .retry_after
            .is_some_and(|when| std::time::Instant::now() < when)
        {
            return Temperature::unavailable(&self.last_error);
        }
        let result = (|| {
            if self.session.is_none() {
                let path = self.helper_path.as_ref().ok_or("helper_missing")?;
                self.session = Some(process::Session::start(path)?);
            }
            self.session.as_mut().unwrap().snapshot()
        })();
        match result {
            Ok(value) => value,
            Err(status) => {
                self.session = None;
                self.last_error = status.into();
                self.retry_after =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
                Temperature::unavailable(status)
            }
        }
    }

    #[cfg(not(windows))]
    pub fn read(&mut self) -> Temperature {
        // Native OS components do not require the optional Windows provider.
        let now = std::time::Instant::now();
        if self.discovery_after.is_none_or(|when| now >= when) {
            self.components.refresh_list();
            self.discovery_after = Some(now + std::time::Duration::from_secs(600));
        } else {
            self.components.refresh();
        }
        let mut value: Option<f32> = None;
        let mut labels = Vec::new();
        for component in &self.components {
            let label = component.label();
            if !(label.starts_with("coretemp Package id ")
                || label == "k10temp Tdie"
                || label == "CPU Die")
            {
                continue;
            }
            let current = component.temperature();
            if current.is_finite() && current > 0.0 && current < 150.0 {
                value = Some(value.map_or(current, |old| old.max(current)));
                labels.push(label.to_string());
            }
        }
        Temperature {
            value,
            status: if value.is_some() { "ok" } else { "unsupported" }.into(),
            source: Some("native/sysinfo".into()),
            labels,
        }
    }
}

#[cfg(windows)]
mod process {
    use super::{parse_snapshot, Temperature};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    const MAX_LINE: u64 = 32 * 1024;
    pub(super) struct Session {
        child: Child,
        stdin: ChildStdin,
        responses: Receiver<Result<Vec<u8>, &'static str>>,
        request_id: u64,
    }

    impl Session {
        pub(super) fn start(path: &Path) -> Result<Self, &'static str> {
            if !path.is_file() {
                return Err("helper_missing");
            }
            let mut command = Command::new(path);
            command
                .arg("--serve")
                .env("DOTNET_DISABLE_GUI_ERRORS", "1")
                .current_dir(path.parent().ok_or("helper_missing")?);
            Self::start_command(command)
        }

        fn start_command(mut command: Command) -> Result<Self, &'static str> {
            let mut child = command
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| "helper_start_failed")?;
            let stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let (sender, responses) = mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    let read = Read::by_ref(&mut reader)
                        .take(MAX_LINE + 1)
                        .read_until(b'\n', &mut line);
                    if read.is_err()
                        || line.is_empty()
                        || line.len() as u64 > MAX_LINE
                        || !line.ends_with(b"\n")
                    {
                        let _ = sender.send(Err("invalid_response"));
                        break;
                    }
                    if sender.send(Ok(line)).is_err() {
                        break;
                    }
                }
            });
            Ok(Self {
                child,
                stdin,
                responses,
                request_id: 0,
            })
        }

        pub(super) fn snapshot(&mut self) -> Result<Temperature, &'static str> {
            self.snapshot_with_timeout(Duration::from_secs(3))
        }

        fn snapshot_with_timeout(
            &mut self,
            timeout: Duration,
        ) -> Result<Temperature, &'static str> {
            self.request_id += 1;
            writeln!(
                self.stdin,
                "{{\"protocol\":1,\"request_id\":{}}}",
                self.request_id
            )
            .map_err(|_| "helper_exited")?;
            let bytes = self
                .responses
                .recv_timeout(timeout)
                .map_err(|_| "provider_timeout")??;
            parse_snapshot(&bytes, self.request_id)
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn fixture(script: &str) -> Session {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
            Session::start_command(command).unwrap()
        }

        #[test]
        fn one_process_serves_multiple_snapshots() {
            let mut session = fixture(
                r#"while ($line = [Console]::ReadLine()) {
                $request = $line | ConvertFrom-Json
                [Console]::WriteLine((@{protocol=1;request_id=$request.request_id;
                    provider='librehardwaremonitor';provider_version='0.9.6';
                    status='unsupported';readings=@()} | ConvertTo-Json -Compress))
            }"#,
            );
            let pid = session.child.id();
            assert_eq!(
                session
                    .snapshot_with_timeout(Duration::from_secs(15))
                    .unwrap()
                    .status,
                "unsupported"
            );
            assert_eq!(
                session
                    .snapshot_with_timeout(Duration::from_secs(15))
                    .unwrap()
                    .status,
                "unsupported"
            );
            assert_eq!(session.child.id(), pid);
        }

        #[test]
        fn hung_provider_has_bounded_response_wait() {
            let mut session = fixture("$null = [Console]::ReadLine(); Start-Sleep -Seconds 30");
            let start = std::time::Instant::now();
            assert_eq!(session.snapshot().unwrap_err(), "provider_timeout");
            assert!(start.elapsed() < Duration::from_secs(5));
        }

        #[test]
        fn oversized_output_is_rejected_without_waiting_for_eof() {
            let mut session = fixture("$null = [Console]::ReadLine(); [Console]::WriteLine('x' * 40000); Start-Sleep -Seconds 30");
            assert_eq!(
                session
                    .snapshot_with_timeout(Duration::from_secs(15))
                    .unwrap_err(),
                "invalid_response"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(readings: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"protocol":1,"request_id":7,
            "provider":"librehardwaremonitor","provider_version":"0.9.6",
            "status":"ok","readings":readings}))
        .unwrap()
    }

    #[test]
    fn selects_real_packages_and_rejects_gpu_and_core_fallbacks() {
        let bytes = response(serde_json::json!([
            {"hardware_id":"/intelcpu/0","label":"CPU Package","celsius":51.0},
            {"hardware_id":"/intelcpu/0","label":"CPU Core #1","celsius":95.0},
            {"hardware_id":"/amdcpu/0","label":"Core (Tctl/Tdie)","celsius":90.0},
            {"hardware_id":"/amdcpu/0","label":"Core (Tdie)","celsius":62.0},
            {"hardware_id":"/gpu/0","label":"CPU Package","celsius":99.0}
        ]));
        assert_eq!(parse_snapshot(&bytes, 7).unwrap().value, Some(62.0));
        assert!(parse_snapshot(&bytes, 8).is_err());
    }

    #[test]
    fn rejects_unversioned_legacy_and_invalid_responses() {
        assert!(parse_snapshot(b"{}", 7).is_err());
        let legacy = String::from_utf8(response(serde_json::json!([])))
            .unwrap()
            .replace("0.9.6", "0.9.4");
        assert!(parse_snapshot(legacy.as_bytes(), 7).is_err());
        let bytes = response(serde_json::json!([
            {"hardware_id":"/intelcpu/0","label":"CPU Package","celsius":0.0},
            {"hardware_id":"/intelcpu/1","label":"CPU Package","celsius":999.0}
        ]));
        assert_eq!(parse_snapshot(&bytes, 7).unwrap().value, None);
    }

    #[cfg(windows)]
    #[test]
    fn disabled_provider_never_starts_and_missing_helper_is_unknown() {
        let mut reader = TemperatureReader::new(None, false);
        assert_eq!(reader.read().status, "disabled");
        reader.set_enabled(true);
        assert_eq!(reader.read().status, "helper_missing");
        assert_eq!(reader.read().value, None);
        reader.set_enabled(false);
        assert_eq!(reader.read().status, "disabled");
    }
}
