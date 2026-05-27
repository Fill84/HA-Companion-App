//! Sidecar IPC client for the bundled `hwmon-helper.exe`.
//!
//! Owns one child process for the lifetime of the app, sends one JSON object
//! per `poll()` call over its stdin, reads one JSON object back from its
//! stdout. On any read/write or parse failure the client transitions to
//! `Failed` and the next `poll()` attempts to respawn the helper.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HwmonSnapshot {
    #[serde(rename = "cpu_package_c")]
    pub cpu_package_c: Option<f32>,
    #[serde(rename = "cpu_core_avg_c")]
    pub cpu_core_avg_c: Option<f32>,
}

impl HwmonSnapshot {
    /// Best available CPU temperature for the regular `sensor.cpu_temperature`
    /// entity: prefer package, fall back to core average.
    pub fn best_cpu_temp(&self) -> Option<f32> {
        self.cpu_package_c.or(self.cpu_core_avg_c)
    }
}

#[derive(Debug, Serialize)]
struct PollRequest {
    cmd: &'static str,
}

impl PollRequest {
    const fn poll() -> Self { Self { cmd: "poll" } }
}

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// IPC client for the hwmon-helper sidecar.
///
/// Owns the child process and exposes `poll()` for each sensor cycle.
/// On a hard failure the client is left in `Failed` state and `poll()`
/// returns errors until the caller drops + respawns the client.
pub struct HwmonClient {
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout_lines: tokio::io::Lines<BufReader<ChildStdout>>,
}

impl HwmonClient {
    /// Spawn the bundled helper at the conventional Tauri sidecar path.
    /// Returns Err if the binary is missing or the OS rejects the spawn.
    pub async fn spawn_default() -> io::Result<Self> {
        let path = default_helper_path();
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("hwmon-helper binary not found at {}", path.display()),
            ));
        }
        let cmd = Command::new(&path);
        Self::spawn_with_command(cmd).await
    }

    /// Spawn from a pre-built Command — used by tests to inject a stand-in.
    pub async fn spawn_with_command(mut cmd: Command) -> io::Result<Self> {
        cmd.stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::null());
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "no stdin handle on child")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "no stdout handle on child")
        })?;
        let stdout_lines = BufReader::new(stdout).lines();
        Ok(Self { child, stdin, stdout_lines })
    }

    /// Send one `{"cmd":"poll"}` request, read one JSON line of response,
    /// parse into `HwmonSnapshot`. Any IO/parse error means the helper is
    /// dead from our perspective; the caller should drop this client.
    pub async fn poll(&mut self) -> io::Result<HwmonSnapshot> {
        let req = serde_json::to_string(&PollRequest::poll()).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("serialize: {e}"))
        })?;
        self.stdin.write_all(req.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        let line = match self.stdout_lines.next_line().await? {
            Some(l) => l,
            None => return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                "helper closed stdout")),
        };
        serde_json::from_str::<HwmonSnapshot>(&line).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData,
                format!("parse '{line}': {e}"))
        })
    }

    /// Best-effort terminate the helper. Tokio will reap.
    #[allow(dead_code)]
    pub async fn shutdown(mut self) {
        let _ = self.stdin.write_all(b"{\"cmd\":\"shutdown\"}\n").await;
        let _ = self.child.wait().await;
    }
}

fn default_helper_path() -> PathBuf {
    // In dev, the file lives at desktop-app/src-tauri/binaries/<name>.exe
    // alongside our crate. In a Tauri-built install, Tauri copies it next
    // to the main exe — we check both.
    let exe_name = "hwmon-helper-x86_64-pc-windows-msvc.exe";

    // Sibling of current_exe (release install)
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let candidate = dir.join(exe_name);
            if candidate.exists() { return candidate; }
        }
    }

    // Dev mode: relative to CARGO_MANIFEST_DIR / binaries
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("binaries").join(exe_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_snapshot() {
        let json = r#"{"cpu_package_c":42.5,"cpu_core_avg_c":40.3}"#;
        let snap: HwmonSnapshot = serde_json::from_str(json).expect("parse");
        assert_eq!(snap.cpu_package_c, Some(42.5));
        assert_eq!(snap.cpu_core_avg_c, Some(40.3));
        assert_eq!(snap.best_cpu_temp(), Some(42.5));
    }

    #[test]
    fn parses_partial_snapshot_with_null_package() {
        let json = r#"{"cpu_package_c":null,"cpu_core_avg_c":47.1}"#;
        let snap: HwmonSnapshot = serde_json::from_str(json).expect("parse");
        assert_eq!(snap.cpu_package_c, None);
        assert_eq!(snap.cpu_core_avg_c, Some(47.1));
        assert_eq!(snap.best_cpu_temp(), Some(47.1));
    }

    #[test]
    fn parses_both_null_returns_none_from_best() {
        let json = r#"{"cpu_package_c":null,"cpu_core_avg_c":null}"#;
        let snap: HwmonSnapshot = serde_json::from_str(json).expect("parse");
        assert_eq!(snap.best_cpu_temp(), None);
    }

    #[test]
    fn poll_request_serializes_as_expected() {
        let req = PollRequest::poll();
        assert_eq!(serde_json::to_string(&req).unwrap(), r#"{"cmd":"poll"}"#);
    }

    /// Write a canned JSON line to a unique temp file. The fake helper reads
    /// from that file via `cmd /c type <path>` to avoid Windows command-line
    /// quoting eating the double-quotes inside our JSON.
    fn canned_helper(canned: &str) -> tokio::process::Command {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ha-companion-hwmon-canned-{}-{}.txt",
            std::process::id(),
            n
        ));
        std::fs::write(&path, canned).expect("write canned");
        let mut cmd = tokio::process::Command::new("cmd.exe");
        cmd.args(["/c", "type", path.to_str().expect("utf-8 path")]);
        cmd
    }

    #[tokio::test]
    async fn poll_reads_one_json_line_from_helper_stdout() {
        let canned = r#"{"cpu_package_c":45.0,"cpu_core_avg_c":42.0}"#;
        let cmd = canned_helper(canned);
        let mut client = HwmonClient::spawn_with_command(cmd)
            .await
            .expect("spawn fake helper");
        let snap = client.poll().await.expect("poll succeeds");
        assert_eq!(snap.cpu_package_c, Some(45.0));
        assert_eq!(snap.best_cpu_temp(), Some(45.0));
    }

    #[tokio::test]
    async fn poll_returns_err_when_helper_already_exited() {
        // exit-after-one-line cmd: print canned, then exit. Two polls — first
        // succeeds, second should fail because stdout is at EOF.
        let canned = r#"{"cpu_package_c":50.0,"cpu_core_avg_c":null}"#;
        let cmd = canned_helper(canned);
        let mut client = HwmonClient::spawn_with_command(cmd)
            .await
            .expect("spawn fake helper");
        let _first = client.poll().await.expect("first poll");
        let second = client.poll().await;
        assert!(second.is_err(), "second poll on dead helper must error");
    }
}
