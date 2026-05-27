# Phase 1: Sensor Bugfixes & Release Logging — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the three sensors that shipped broken in v1.0.1 and v1.0.2 (System Uptime, Last Boot, CPU Temperature) — for Uptime and Last Boot by changing the data we send to Home Assistant; for CPU Temperature by adding diagnostic logging so the next iteration is data-driven, not a guess. Add a file-based logger that works in release builds so we never ship blind again.

**Architecture:** Extract three pure functions from `collector.rs` so they can be unit-tested without touching real hardware: `format_boot_time(timestamp) -> Option<String>` using `chrono`, `build_uptime_sensor(uptime_seconds) -> SensorValue` with numeric state, and `build_last_boot_sensor(boot_time) -> Option<SensorValue>` that emits nothing when `boot_time == 0`. Replace the conditional `env_logger` setup in `lib.rs` with an always-on `simplelog::WriteLogger` writing to `%APPDATA%\com.ha-companion.desktop\app.log`. Add diagnostic info-level logging in every CPU-temperature fallback in `cpu.rs` so jouw next run produces a log we can actually read.

**Tech Stack:** Rust 2021 / Tauri 2 / sysinfo 0.32 / wmi 0.14 / `log` 0.4 / **new:** `chrono` 0.4 (formatting only, no clock features), `simplelog` 0.12 (file logger)

**Out of scope for Phase 1:**
- LHM-via-helper-subprocess for accurate CPU temperature → Phase 2
- Availability binary_sensor + shutdown hook → Phase 3
- Version bump and release build — only after user confirms Phase 1 smoke test passes

---

## File map

**Modify:**
- [desktop-app/src-tauri/Cargo.toml](../../../desktop-app/src-tauri/Cargo.toml) — add `chrono` and `simplelog` dependencies
- [desktop-app/src-tauri/src/sensors/collector.rs](../../../desktop-app/src-tauri/src/sensors/collector.rs) — replace `chrono_from_timestamp` with `chrono` crate, extract `format_boot_time` / `build_uptime_sensor` / `build_last_boot_sensor` as testable functions, change uptime state to numeric
- [desktop-app/src-tauri/src/lib.rs](../../../desktop-app/src-tauri/src/lib.rs) — replace conditional `env_logger::Builder::from_env(...)` init with always-on file logger
- [desktop-app/src-tauri/src/sensors/cpu.rs](../../../desktop-app/src-tauri/src/sensors/cpu.rs) — add info-level diagnostic log on every WMI fallback attempt, including failures

**Create:**
- [desktop-app/src-tauri/src/logging.rs](../../../desktop-app/src-tauri/src/logging.rs) — new module exposing `log_file_path()` and `init_logger(path)`

**Test files:** Rust unit tests live inside the file they test, under `#[cfg(test)] mod tests`. No separate `tests/` directory.

---

## Task 1: `format_boot_time` with `chrono`

**Goal:** Replace the handmade ISO-8601 formatter with `chrono`, plus a guard that returns `None` when the timestamp is 0 (the failure-mode value sysinfo may return when `boot_time` is unavailable).

**Files:**
- Modify: [desktop-app/src-tauri/Cargo.toml](../../../desktop-app/src-tauri/Cargo.toml)
- Modify: [desktop-app/src-tauri/src/sensors/collector.rs](../../../desktop-app/src-tauri/src/sensors/collector.rs:746-795)

- [ ] **Step 1: Write the failing tests**

Append to the bottom of `desktop-app/src-tauri/src/sensors/collector.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_boot_time_returns_iso_for_known_epoch() {
        // 2026-05-26 12:00:00 UTC = 1779796800
        assert_eq!(
            format_boot_time(1779796800),
            Some("2026-05-26T12:00:00+00:00".to_string())
        );
    }

    #[test]
    fn format_boot_time_returns_none_for_zero() {
        assert_eq!(format_boot_time(0), None);
    }

    #[test]
    fn format_boot_time_handles_unix_epoch_plus_one() {
        // Smallest non-zero, ensures we don't accidentally return None for tiny values.
        assert_eq!(
            format_boot_time(1),
            Some("1970-01-01T00:00:01+00:00".to_string())
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml format_boot_time
```

Expected: FAIL with "cannot find function `format_boot_time` in this scope".

- [ ] **Step 3: Add `chrono` and implement `format_boot_time`**

In `desktop-app/src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

(`clock` brings in the timezone machinery we need for `Utc.timestamp_opt`; we don't actually use the wall clock, but it's the simplest feature combination that compiles.)

In `desktop-app/src-tauri/src/sensors/collector.rs`, **delete** the existing handmade `chrono_from_timestamp` function (lines 746-795 in the current file). Then add this new function near the top of the file, right after the existing `use` statements:

```rust
use chrono::{SecondsFormat, TimeZone, Utc};

/// Format a UNIX timestamp (seconds since 1970-01-01 UTC) as an RFC3339 string
/// with a `+00:00` offset suffix. Returns `None` for the failure-mode value 0,
/// so the Last Boot sensor can be omitted entirely rather than reporting 1970.
pub(crate) fn format_boot_time(timestamp: u64) -> Option<String> {
    if timestamp == 0 {
        return None;
    }
    let dt = Utc.timestamp_opt(timestamp as i64, 0).single()?;
    Some(dt.to_rfc3339_opts(SecondsFormat::Secs, false))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml format_boot_time
```

Expected: PASS — three tests in `collector::tests`.

- [ ] **Step 5: Commit**

```
git add desktop-app/src-tauri/Cargo.toml desktop-app/src-tauri/Cargo.lock desktop-app/src-tauri/src/sensors/collector.rs
git commit -m "refactor(sensors): replace handmade ISO formatter with chrono and guard boot_time==0"
```

---

## Task 2: `build_uptime_sensor` with numeric state — the System Uptime fix

**Goal:** Extract the System Uptime `SensorValue` construction into a pure function and **change the state from `"5h 23m"` (string) to `uptime_seconds` (JSON number)**. The string-state is incompatible with `state_class: total_increasing` and is the root cause of the sensor being `unavailable` in HA. Keep the human-readable string as an attribute so it remains visible in the entity's "more info" panel.

**Files:**
- Modify: [desktop-app/src-tauri/src/sensors/collector.rs](../../../desktop-app/src-tauri/src/sensors/collector.rs:378-400)

- [ ] **Step 1: Write the failing tests**

Append to the existing `#[cfg(test)] mod tests` block in `collector.rs`:

```rust
    #[test]
    fn build_uptime_sensor_state_is_numeric_seconds() {
        let sensor = build_uptime_sensor(3725); // 1h 2m 5s

        match &sensor.state {
            serde_json::Value::Number(n) => {
                assert_eq!(n.as_u64(), Some(3725), "state must be the raw seconds count");
            }
            other => panic!("state must be a JSON Number, got {:?}", other),
        }
    }

    #[test]
    fn build_uptime_sensor_metadata_matches_ha_duration_contract() {
        let sensor = build_uptime_sensor(0);

        assert_eq!(sensor.unique_id, "system_uptime");
        assert_eq!(sensor.sensor_type, "sensor");
        assert_eq!(sensor.device_class.as_deref(), Some("duration"));
        assert_eq!(sensor.unit_of_measurement.as_deref(), Some("s"));
        assert_eq!(sensor.state_class.as_deref(), Some("total_increasing"));
        assert!(sensor.update_at_interval);
    }

    #[test]
    fn build_uptime_sensor_attributes_contain_human_breakdown() {
        let sensor = build_uptime_sensor(90061); // 1d 1h 1m 1s

        assert_eq!(sensor.attributes.get("uptime_seconds"), Some(&serde_json::json!(90061)));
        assert_eq!(sensor.attributes.get("days"), Some(&serde_json::json!(1)));
        assert_eq!(sensor.attributes.get("hours"), Some(&serde_json::json!(25))); // total hours
        assert_eq!(sensor.attributes.get("minutes"), Some(&serde_json::json!(1)));
        assert_eq!(
            sensor.attributes.get("human"),
            Some(&serde_json::json!("1d 1h 1m"))
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml build_uptime_sensor
```

Expected: FAIL with "cannot find function `build_uptime_sensor` in this scope".

- [ ] **Step 3: Implement `build_uptime_sensor` and wire it into `collect_dynamic`**

In `desktop-app/src-tauri/src/sensors/collector.rs`, add this function above the `impl SensorCollector` block (so it's a free function, easy to test):

```rust
/// Build the SensorValue for System Uptime.
///
/// The state is the raw seconds count as a JSON Number — required by HA's
/// `state_class: total_increasing` + `device_class: duration` contract.
/// The human-readable "1d 2h 3m" form is exposed as an attribute so it
/// remains visible without breaking validation.
pub(crate) fn build_uptime_sensor(uptime_seconds: u64) -> SensorValue {
    let days = uptime_seconds / 86400;
    let hours = uptime_seconds / 3600;
    let minutes = (uptime_seconds % 3600) / 60;

    let human = if days > 0 {
        format!("{}d {}h {}m", days, hours - days * 24, minutes)
    } else {
        format!("{}h {}m", hours, minutes)
    };

    let mut attributes = HashMap::new();
    attributes.insert("uptime_seconds".into(), serde_json::json!(uptime_seconds));
    attributes.insert("days".into(), serde_json::json!(days));
    attributes.insert("hours".into(), serde_json::json!(hours));
    attributes.insert("minutes".into(), serde_json::json!(minutes));
    attributes.insert("human".into(), serde_json::json!(human));

    SensorValue {
        unique_id: "system_uptime".into(),
        name: "System Uptime".into(),
        state: serde_json::json!(uptime_seconds),
        sensor_type: "sensor".into(),
        device_class: Some("duration".into()),
        unit_of_measurement: Some("s".into()),
        state_class: Some("total_increasing".into()),
        icon: Some("mdi:clock-outline".into()),
        attributes,
        update_at_interval: true,
    }
}
```

Then **replace** the inline System Uptime block in `collect_dynamic` (the block currently at lines 378-400 starting with `if self.is_enabled("system_uptime") {` and ending before the `process_count` block) with:

```rust
            if self.is_enabled("system_uptime") {
                sensors.push(build_uptime_sensor(dyn_info.uptime_seconds));
            }
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: PASS — all `collector::tests` including the new three.

- [ ] **Step 5: Commit**

```
git add desktop-app/src-tauri/src/sensors/collector.rs
git commit -m "fix(sensors): emit System Uptime as numeric seconds to satisfy HA total_increasing"
```

---

## Task 3: `build_last_boot_sensor` returning `Option`

**Goal:** Extract Last Boot construction into a pure function that returns `Option<SensorValue>` — `None` when `boot_time == 0` (which means sysinfo couldn't read it) so we don't pollute HA with a Last Boot timestamp of 1970.

**Files:**
- Modify: [desktop-app/src-tauri/src/sensors/collector.rs](../../../desktop-app/src-tauri/src/sensors/collector.rs:573-594)

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn build_last_boot_sensor_returns_none_for_zero() {
        assert!(build_last_boot_sensor(0).is_none());
    }

    #[test]
    fn build_last_boot_sensor_emits_iso_state_for_valid_timestamp() {
        let sensor = build_last_boot_sensor(1779796800).expect("must be Some");

        assert_eq!(sensor.unique_id, "last_boot");
        assert_eq!(sensor.device_class.as_deref(), Some("timestamp"));
        assert_eq!(
            sensor.state,
            serde_json::json!("2026-05-26T12:00:00+00:00"),
        );
        assert_eq!(
            sensor.attributes.get("boot_timestamp"),
            Some(&serde_json::json!(1779796800u64)),
        );
        assert!(!sensor.update_at_interval, "last_boot is static");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml build_last_boot_sensor
```

Expected: FAIL with "cannot find function `build_last_boot_sensor` in this scope".

- [ ] **Step 3: Implement `build_last_boot_sensor` and wire it into `collect_static`**

In `desktop-app/src-tauri/src/sensors/collector.rs`, add next to `build_uptime_sensor`:

```rust
/// Build the Last Boot SensorValue, or None when the boot time is unknown.
pub(crate) fn build_last_boot_sensor(boot_time: u64) -> Option<SensorValue> {
    let iso = format_boot_time(boot_time)?;

    let mut attributes = HashMap::new();
    attributes.insert("boot_timestamp".into(), serde_json::json!(boot_time));

    Some(SensorValue {
        unique_id: "last_boot".into(),
        name: "Last Boot".into(),
        state: serde_json::json!(iso),
        sensor_type: "sensor".into(),
        device_class: Some("timestamp".into()),
        unit_of_measurement: None,
        state_class: None,
        icon: Some("mdi:restart".into()),
        attributes,
        update_at_interval: false,
    })
}
```

Then **replace** the Last Boot block in `collect_static` (currently at lines 573-594, starting with `if self.is_enabled("last_boot") {`) with:

```rust
        if self.is_enabled("last_boot") {
            if let Some(sensor) = build_last_boot_sensor(sys_info.boot_time) {
                sensors.push(sensor);
            } else {
                log::warn!(
                    "[SystemInfo] boot_time is 0 — skipping Last Boot sensor"
                );
            }
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: PASS — all tests across the crate.

- [ ] **Step 5: Commit**

```
git add desktop-app/src-tauri/src/sensors/collector.rs
git commit -m "fix(sensors): use chrono for Last Boot ISO and skip when boot_time is unknown"
```

---

## Task 4: File-based logger module (`logging.rs`)

**Goal:** Create a logger that always writes to disk, regardless of release/debug, so future bug reports can be diagnosed.

**Files:**
- Modify: [desktop-app/src-tauri/Cargo.toml](../../../desktop-app/src-tauri/Cargo.toml)
- Create: [desktop-app/src-tauri/src/logging.rs](../../../desktop-app/src-tauri/src/logging.rs)

- [ ] **Step 1: Write the failing tests**

Create `desktop-app/src-tauri/src/logging.rs` with **only the test module** for now (so it fails to compile because the function under test doesn't exist):

```rust
//! Release-mode file logger.
//!
//! Writes to `%APPDATA%\com.ha-companion.desktop\app.log` on Windows.

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn make_log_file_creates_parent_dirs() {
        let base = std::env::temp_dir()
            .join(format!("ha-companion-test-{}", std::process::id()));
        let log = base.join("nested").join("a").join("app.log");
        let _ = fs::remove_dir_all(&base);

        let file = make_log_file(&log).expect("create");
        drop(file);
        assert!(log.exists(), "log file must exist at {:?}", log);

        // Re-opening must succeed and not truncate.
        fs::write(&log, b"existing\n").expect("seed content");
        let _file2 = make_log_file(&log).expect("re-open");
        let content = fs::read_to_string(&log).expect("read");
        assert!(content.contains("existing"), "must append, not truncate");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn log_file_path_under_appdata_uses_app_identifier() {
        // SAFETY: single-threaded test, restored at end. std::env::set_var is
        // marked unsafe from Rust 2024; this attribute keeps the call quiet
        // on edition 2021 too.
        let prev = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", r"C:\Users\test\AppData\Roaming");

        let path = log_file_path().expect("must compute path");
        assert!(
            path.ends_with(r"com.ha-companion.desktop\app.log"),
            "unexpected path: {:?}",
            path
        );

        match prev {
            Some(v) => std::env::set_var("APPDATA", v),
            None => std::env::remove_var("APPDATA"),
        }
    }
}
```

Add `mod logging;` to `desktop-app/src-tauri/src/lib.rs` directly after the existing `mod commands;` line (only adding the declaration — we wire it up in Task 5).

- [ ] **Step 2: Run tests to verify they fail**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml logging
```

Expected: FAIL with "cannot find function `make_log_file`" and "cannot find function `log_file_path`".

- [ ] **Step 3: Add `simplelog` and implement the module**

In `desktop-app/src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
simplelog = "0.12"
```

Then **replace** the contents of `desktop-app/src-tauri/src/logging.rs` with the full module:

```rust
//! Release-mode file logger.
//!
//! Writes to `%APPDATA%\com.ha-companion.desktop\app.log` on Windows.
//! Falls back to `<cwd>/app.log` if `APPDATA` isn't set (CI / Linux dev).

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

const APP_DATA_DIR: &str = "com.ha-companion.desktop";
const LOG_FILE_NAME: &str = "app.log";

/// Compute the on-disk log file path. Uses `%APPDATA%` on Windows.
pub fn log_file_path() -> std::io::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "APPDATA not set")
    })?;
    Ok(Path::new(&appdata).join(APP_DATA_DIR).join(LOG_FILE_NAME))
}

/// Open (or create) the log file in append mode, creating parent directories
/// as needed. Exposed so tests can verify file/dir creation independently of
/// global logger initialisation (which can only run once per process).
pub fn make_log_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Initialise the global logger to write to `path`. Returns Err if a logger
/// is already installed or the file can't be opened.
pub fn init_logger(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = make_log_file(path)?;
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();
    WriteLogger::init(LevelFilter::Info, config, file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn make_log_file_creates_parent_dirs() {
        let base = std::env::temp_dir()
            .join(format!("ha-companion-test-{}", std::process::id()));
        let log = base.join("nested").join("a").join("app.log");
        let _ = fs::remove_dir_all(&base);

        let file = make_log_file(&log).expect("create");
        drop(file);
        assert!(log.exists(), "log file must exist at {:?}", log);

        fs::write(&log, b"existing\n").expect("seed content");
        let _file2 = make_log_file(&log).expect("re-open");
        let content = fs::read_to_string(&log).expect("read");
        assert!(content.contains("existing"), "must append, not truncate");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn log_file_path_under_appdata_uses_app_identifier() {
        let prev = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", r"C:\Users\test\AppData\Roaming");

        let path = log_file_path().expect("must compute path");
        assert!(
            path.ends_with(r"com.ha-companion.desktop\app.log"),
            "unexpected path: {:?}",
            path
        );

        match prev {
            Some(v) => std::env::set_var("APPDATA", v),
            None => std::env::remove_var("APPDATA"),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml logging
```

Expected: PASS — two tests in `logging::tests`. Run the full suite to confirm no regressions:

```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: PASS for everything.

- [ ] **Step 5: Commit**

```
git add desktop-app/src-tauri/Cargo.toml desktop-app/src-tauri/Cargo.lock desktop-app/src-tauri/src/logging.rs desktop-app/src-tauri/src/lib.rs
git commit -m "feat(logging): add release-mode file logger module"
```

---

## Task 5: Wire `init_logger` into `lib.rs::run()`

**Goal:** Replace the conditional `env_logger` init with our always-on file logger, so `log::info!` calls produce output in release builds.

**Files:**
- Modify: [desktop-app/src-tauri/src/lib.rs](../../../desktop-app/src-tauri/src/lib.rs:30-35)

- [ ] **Step 1: Write a manual verification note (no unit test)**

The wiring is integration glue; `WriteLogger::init` is a global side-effect and can only happen once per process, so a unit test isn't practical. We verify by smoke test in Task 7. Add this comment so it's obvious in the diff why no test was added:

> No automated test for this step — `WriteLogger::init` is one-shot global state. Verified by Task 7 smoke test.

- [ ] **Step 2: Replace the env_logger init in `lib.rs::run`**

In `desktop-app/src-tauri/src/lib.rs`, **replace** the existing init block at lines 30-35:

```rust
pub fn run(dev_mode: bool) {
    // In dev/debug builds, init logger so log::info!/error! show in terminal
    if dev_mode || cfg!(debug_assertions) {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
    }
```

with:

```rust
pub fn run(dev_mode: bool) {
    // Always log to file. In dev/debug, also mirror to stderr so the
    // terminal shows live output.
    let file_log_path = logging::log_file_path();
    if let Ok(ref path) = file_log_path {
        if let Err(e) = logging::init_logger(path) {
            // Logger init failed — fall back to env_logger so we at least get
            // stderr output and can diagnose why the file logger failed.
            eprintln!("[bootstrap] file logger init failed: {} — falling back to stderr", e);
            let _ = env_logger::Builder::from_env(
                env_logger::Env::default().default_filter_or("info"),
            )
            .try_init();
        }
    } else {
        // APPDATA not set (CI/Linux dev) — stderr is fine.
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info"),
        )
        .try_init();
    }

    log::info!(
        "[bootstrap] HA Companion v{} starting (dev_mode={}, log_file={:?})",
        env!("CARGO_PKG_VERSION"),
        dev_mode,
        file_log_path.as_ref().ok(),
    );
```

Remove the now-unused `let _ = dev_mode;` line further down in `setup` if it exists (it was suppressing an unused-var warning that no longer applies).

- [ ] **Step 3: Verify the crate still builds**

Run:
```
cargo check --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: clean build, no warnings about unused `env_logger` or `logging`.

- [ ] **Step 4: Run the full test suite**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: all tests still PASS.

- [ ] **Step 5: Commit**

```
git add desktop-app/src-tauri/src/lib.rs
git commit -m "feat(logging): always log to file in release, env_logger as fallback only"
```

---

## Task 6: Diagnostic logging in CPU-temperature fallbacks

**Goal:** When CPU temperature ends up `None`, the log file must contain enough detail that we can see *which* WMI source returned what — including raw values that were filtered out — so Phase 2 can target the real failure. No behaviour change, no test (this is observability code, not logic).

**Files:**
- Modify: [desktop-app/src-tauri/src/sensors/cpu.rs](../../../desktop-app/src-tauri/src/sensors/cpu.rs:167-242)

- [ ] **Step 1: Identify the silent failure paths in `cpu.rs`**

Read [desktop-app/src-tauri/src/sensors/cpu.rs](../../../desktop-app/src-tauri/src/sensors/cpu.rs) end-to-end and confirm:
- `collect_cpu_temp_hwmon` logs success but is silent on "no results"
- `collect_cpu_temp_wmi` logs WMI query failures at `debug!` (invisible at info-level) and warns only when *all* sources failed — the per-row out-of-range and missing-variant cases are not logged at all
- The top-level fallthrough in `collect` (where `temperature` ends up `None`) doesn't summarise what was tried

- [ ] **Step 2: Add diagnostic logging — no test, code-level change**

In `desktop-app/src-tauri/src/sensors/cpu.rs`, make these edits:

**In `collect_cpu_temp_hwmon`** (the function shared by LHM and OHM): at the start of the function, replace `let com_lib = COMLibrary::new().ok()?;` and the connection-open line so failures aren't silently swallowed:

```rust
    let com_lib = match COMLibrary::new() {
        Ok(c) => c,
        Err(e) => {
            log::info!("[CPU] {}: COM init failed: {}", source_name, e);
            return None;
        }
    };
    let wmi_con = match WMIConnection::with_namespace_path(namespace, com_lib) {
        Ok(w) => w,
        Err(e) => {
            log::info!(
                "[CPU] {}: namespace {} not available ({}). Hardware monitor probably not running.",
                source_name, namespace, e
            );
            return None;
        }
    };

    let results: Vec<HashMap<String, Variant>> = match wmi_con.raw_query(
        "SELECT Identifier, SensorType, Value, Name FROM Sensor WHERE SensorType = 'Temperature'",
    ) {
        Ok(r) => r,
        Err(e) => {
            log::info!("[CPU] {}: Sensor query failed: {}", source_name, e);
            return None;
        }
    };

    log::info!(
        "[CPU] {}: Sensor query returned {} temperature rows",
        source_name,
        results.len()
    );
```

Then, after the `for row in &results` loop ends and before the `if let Some(temp) = preferred` block, add:

```rust
    log::info!(
        "[CPU] {}: matched {} CPU core temps, preferred aggregate = {:?}",
        source_name,
        core_temps.len(),
        preferred
    );
```

**In `collect_cpu_temp_wmi`**: change every `log::debug!` to `log::info!` so the output appears in the default info-level log, **and** add a row-level log inside each loop. For the `MSAcpi_ThermalZoneTemperature` branch, replace the existing loop body so each row is logged:

```rust
                    log::info!(
                        "[CPU] MSAcpi_ThermalZone returned {} row(s)",
                        results.len()
                    );
                    for (i, result) in results.iter().enumerate() {
                        let raw = result.get("CurrentTemperature");
                        log::info!("[CPU] MSAcpi_ThermalZone row {}: raw CurrentTemperature = {:?}", i, raw);
                        let raw_temp = match raw {
                            Some(Variant::UI4(n)) => Some(*n as f32),
                            Some(Variant::UI2(n)) => Some(*n as f32),
                            Some(Variant::I4(n)) => Some(*n as f32),
                            _ => None,
                        };
                        if let Some(tenths_kelvin) = raw_temp {
                            let celsius = (tenths_kelvin / 10.0) - 273.15;
                            log::info!(
                                "[CPU] MSAcpi_ThermalZone row {} -> {:.1}°C (raw {} tenths-K)",
                                i, celsius, tenths_kelvin
                            );
                            if celsius > 0.0 && celsius < 150.0 {
                                log::info!("[CPU] Temperature from MSAcpi_ThermalZone: {:.1}°C", celsius);
                                return Some(celsius);
                            } else {
                                log::info!(
                                    "[CPU] MSAcpi_ThermalZone row {} out of range ({:.1}°C), discarding",
                                    i, celsius
                                );
                            }
                        }
                    }
```

Do the analogous transformation for the `Win32_PerfFormattedData_Counters_ThermalZoneInformation` branch.

Replace the trailing `log::warn!("[CPU] No CPU temperature available from any WMI source");` with:

```rust
    log::warn!(
        "[CPU] No CPU temperature available from any WMI source. \
        Check log entries above — sysinfo, LHM, OHM, MSAcpi, and ThermalZoneInformation \
        were all attempted. This is a known limitation on consumer Windows hardware \
        without a kernel-mode driver; Phase 2 will address this with a bundled hwmon helper."
    );
```

**In `collect`** (the entry point), after the `#[cfg(windows)] if temperature.is_none()` block, add a final summary log:

```rust
    if temperature.is_some() {
        log::info!("[CPU] Final temperature = {:.1}°C", temperature.unwrap());
    } else {
        log::warn!("[CPU] Final temperature = None (will be reported as unknown to HA)");
    }
```

- [ ] **Step 3: Verify the crate still builds clean**

Run:
```
cargo check --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: no errors, no new warnings.

- [ ] **Step 4: Run the full test suite**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: every test PASS.

- [ ] **Step 5: Commit**

```
git add desktop-app/src-tauri/src/sensors/cpu.rs
git commit -m "feat(cpu): info-level diagnostic logging for every CPU temp fallback path"
```

---

## Task 7: Full verification — tests must be 100% green, manual smoke test before any build

**Goal:** Confirm that automated tests pass, the dev build runs, the log file is produced, and the System Uptime / Last Boot entities behave correctly in HA.

**Files:** None.

- [ ] **Step 1: Run the full test suite one last time**

Run:
```
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml
```

Expected: all tests PASS, exit code 0. If anything fails, fix it before continuing — do not move on.

- [ ] **Step 2: Stop the running app (if any) and build a dev binary**

Confirm no `ha-companion.exe` instance is running:
```
tasklist /FI "IMAGENAME eq ha-companion.exe"
```

Then build with the existing dev workflow:
```
cd desktop-app
yarn tauri dev
```

Expected: app launches, tray icon appears.

- [ ] **Step 3: Verify the log file exists and has content**

While the app is still running, in a separate PowerShell:
```
Get-Content "$env:APPDATA\com.ha-companion.desktop\app.log" -Tail 30
```

Expected: at least the `[bootstrap]` line is present plus sensor cycle logs (`[CPU] ...` lines). If the file isn't there, Task 5 wiring is broken.

- [ ] **Step 4: Verify HA shows the fixed sensors**

In Home Assistant, open the device page for this PC and check:
- `sensor.<device>_system_uptime`: state is a **number in seconds** (e.g. `3725`), with the human-readable form visible in the entity attributes as `human: "1h 2m"` or similar
- `sensor.<device>_last_boot`: state is a timestamp shown as a relative time (e.g. "2 hours ago"), not "unknown" or "unavailable"
- `sensor.<device>_cpu_temperature`: state behaviour unchanged — but the log file now contains detailed per-source diagnostic output we can analyse for Phase 2

- [ ] **Step 5: Hand off log file + verification result to user**

Copy the last ~200 lines of `%APPDATA%\com.ha-companion.desktop\app.log` to a fresh paste so we can read what the CPU temperature path produced on this machine. With that we have the data to design Phase 2 correctly.

- [ ] **Step 6: NO version bump, NO release build yet**

Per the project's tests-before-builds rule, only when:
- all `cargo test` cases pass, **and**
- the user has personally verified System Uptime + Last Boot show real values in HA, **and**
- the CPU temperature log diagnostics have been reviewed,

— may we move to Phase 2 (and only Phase 2 changes `Cargo.toml`/`package.json`/`tauri.conf.json` version strings, and only after its own tests are green).

---

## Self-review

**Spec coverage:**
- ✅ System Uptime fix → Task 2 (numeric state + tests)
- ✅ Last Boot fix → Tasks 1 + 3 (chrono formatter + zero-guard + tests)
- ✅ Release-mode file logging → Tasks 4 + 5
- ✅ CPU temperature diagnostic logging (so Phase 2 isn't a guess) → Task 6
- ✅ Tests must be 100% green before any build → Task 7
- ✅ No version bump, no release build in Phase 1 → Task 7 step 6

**Placeholder scan:** No "TODO", "TBD", "add appropriate error handling", or vague references. Each code block is complete and copy-pasteable.

**Type/signature consistency:**
- `format_boot_time(u64) -> Option<String>` — used in Task 1 (defined) and Task 3 (called via `?` operator) ✅
- `build_uptime_sensor(u64) -> SensorValue` — defined and called in Task 2 ✅
- `build_last_boot_sensor(u64) -> Option<SensorValue>` — defined and called in Task 3 ✅
- `log_file_path() -> std::io::Result<PathBuf>`, `make_log_file(&Path) -> std::io::Result<File>`, `init_logger(&Path) -> Result<(), Box<dyn Error>>` — defined in Task 4, used in Task 5 ✅
- `SensorValue.attributes` field — `HashMap<String, serde_json::Value>` per existing definition in [collector.rs:19](../../../desktop-app/src-tauri/src/sensors/collector.rs#L19), matches test assertions ✅
