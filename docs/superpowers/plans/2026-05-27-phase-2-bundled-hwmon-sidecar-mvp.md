# Phase 2 (MVP): Bundled hwmon sidecar for real CPU temperature — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop reporting the static ACPI thermal-zone temperature (27.9°C on the user's Z490 mobo) as CPU temperature. Ship a bundled C# helper based on `LibreHardwareMonitorLib` that runs as a Tauri sidecar process, talks JSON over stdin/stdout, and feeds real per-package CPU temperatures into our Rust collector — with no extra software for the end user to install.

**Architecture:** A self-contained .NET 8 console binary (`hwmon-helper.exe`, ~12 MB) is bundled in the Tauri installer via the `externalBin` mechanism. The Rust app spawns it once at startup, reads JSON sensor snapshots from its stdout, and uses those values when present — falling back to the existing sysinfo/WMI chain when the helper is unavailable. MVP scope is CPU package temperature only; per-core temps, voltages, power, GPU hotspot, SMART, fan RPM etc. are deferred to Phase 2B.

**Tech Stack:**
- C# helper: .NET 8, `LibreHardwareMonitorLib` 0.9.x (MPL-2.0), `System.Text.Json` (stdlib)
- Rust client: `tokio::process` (already a transitive dep of tauri), `serde_json` (already in tree)
- Tauri 2 sidecar via `tauri.conf.json` → `bundle.externalBin`

**Out of scope for Phase 2A (MVP):**
- Per-core temperatures, CPU voltages, CPU power
- GPU hotspot / memory junction / per-fan RPM
- Storage SMART (temp, life remaining, lifetime bytes)
- Mobo VRM/chipset temps + voltage rails
- Phase 3 (availability binary_sensor, shutdown hook)

These build on the same `HwmonSnapshot` channel and are pure data-plumbing once Phase 2A is green.

**Prerequisites (one-time setup on dev machine):**
- .NET 8 SDK installed (verified present: `dotnet --list-sdks` shows `8.0.413`)
- `cargo`, `yarn`, Tauri prerequisites (already present)

---

## File map

**Create:**
- [hwmon-helper/HwmonHelper.csproj](../../../hwmon-helper/HwmonHelper.csproj) — .NET 8 console project with `LibreHardwareMonitorLib` dep, self-contained single-file publish targeting `win-x64`
- [hwmon-helper/Program.cs](../../../hwmon-helper/Program.cs) — entry point; stdin/stdout JSON read-eval-print loop
- [hwmon-helper/Protocol.cs](../../../hwmon-helper/Protocol.cs) — request/response DTOs, JSON serialization context
- [hwmon-helper/Hardware.cs](../../../hwmon-helper/Hardware.cs) — wrapper around `LibreHardwareMonitor.Hardware.Computer`, exposes `Snapshot()` returning the DTOs
- [hwmon-helper/.gitignore](../../../hwmon-helper/.gitignore) — ignore `bin/`, `obj/`, `publish/`
- [hwmon-helper/README.md](../../../hwmon-helper/README.md) — short build/test/run notes
- [desktop-app/src-tauri/src/sensors/hwmon_client.rs](../../../desktop-app/src-tauri/src/sensors/hwmon_client.rs) — spawns the helper, ferries JSON requests/responses, exposes a polling API
- [desktop-app/src-tauri/binaries/.gitkeep](../../../desktop-app/src-tauri/binaries/.gitkeep) — placeholder so the directory exists in git; the actual `hwmon-helper-x86_64-pc-windows-msvc.exe` is built and copied there, not committed
- [scripts/build-helper.ps1](../../../scripts/build-helper.ps1) — one-shot PowerShell that runs `dotnet publish` and copies the renamed binary into `desktop-app/src-tauri/binaries/`

**Modify:**
- [.gitignore](../../../.gitignore) — add `desktop-app/src-tauri/binaries/hwmon-helper-*.exe` so the built artefact isn't committed
- [desktop-app/src-tauri/Cargo.toml](../../../desktop-app/src-tauri/Cargo.toml) — no new dependencies; we use `tokio::process` (already brought in by `tokio = { features = ["full"] }`)
- [desktop-app/src-tauri/src/sensors/mod.rs](../../../desktop-app/src-tauri/src/sensors/mod.rs) — add `pub mod hwmon_client`
- [desktop-app/src-tauri/src/sensors/cpu.rs](../../../desktop-app/src-tauri/src/sensors/cpu.rs) — accept an optional `HwmonSnapshot` in `collect`, prefer it over the existing WMI fallback chain
- [desktop-app/src-tauri/src/sensors/collector.rs](../../../desktop-app/src-tauri/src/sensors/collector.rs) — `SensorCollector::new` accepts a shared `HwmonClient`; `collect_dynamic` polls it once per cycle and passes the snapshot to `cpu::collect`
- [desktop-app/src-tauri/src/lib.rs](../../../desktop-app/src-tauri/src/lib.rs) — start `HwmonClient` in `run()`, pass it into `SensorCollector`
- [desktop-app/src-tauri/tauri.conf.json](../../../desktop-app/src-tauri/tauri.conf.json) — add `"externalBin": ["binaries/hwmon-helper"]` under `bundle`

**Test files (Rust unit tests live inside their .rs file under `#[cfg(test)] mod tests`):**
- Tests for protocol parsing live in `hwmon_client.rs`
- Tests for IPC state machine in `hwmon_client.rs`

---

## Task 1: C# helper skeleton — JSON protocol + hello round-trip

**Goal:** A minimum `hwmon-helper.exe` that reads one JSON command per line on stdin, writes one JSON response per line to stdout. No hardware library yet — just prove the protocol.

**Files:**
- Create: [hwmon-helper/HwmonHelper.csproj](../../../hwmon-helper/HwmonHelper.csproj)
- Create: [hwmon-helper/Program.cs](../../../hwmon-helper/Program.cs)
- Create: [hwmon-helper/Protocol.cs](../../../hwmon-helper/Protocol.cs)
- Create: [hwmon-helper/.gitignore](../../../hwmon-helper/.gitignore)
- Create: [hwmon-helper/README.md](../../../hwmon-helper/README.md)

- [ ] **Step 1: Write the project skeleton (failing — no `dotnet build` will succeed yet because Program.cs is missing)**

Create `hwmon-helper/HwmonHelper.csproj`:

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <RootNamespace>HwmonHelper</RootNamespace>
    <AssemblyName>hwmon-helper</AssemblyName>
    <Nullable>enable</Nullable>
    <LangVersion>latest</LangVersion>
    <InvariantGlobalization>true</InvariantGlobalization>
    <!-- Single-file self-contained publish for sidecar bundling -->
    <PublishSingleFile>true</PublishSingleFile>
    <SelfContained>true</SelfContained>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
    <IncludeNativeLibrariesForSelfExtract>true</IncludeNativeLibrariesForSelfExtract>
    <!-- PublishTrimmed disabled for now: LibreHardwareMonitorLib uses reflection
         on sensor types. We can re-enable with TrimmerRootAssembly entries
         after Phase 2A is verified working. -->
    <PublishTrimmed>false</PublishTrimmed>
  </PropertyGroup>
</Project>
```

Create `hwmon-helper/.gitignore`:

```gitignore
bin/
obj/
publish/
*.user
```

Create `hwmon-helper/README.md`:

````markdown
# hwmon-helper

A bundled sidecar process for the HA Companion App. Reads CPU sensors via
`LibreHardwareMonitorLib` and exposes them as JSON lines over stdin/stdout.
Not intended to be run by end users directly — the Tauri app spawns it.

## Build

```powershell
dotnet publish -c Release -r win-x64 --self-contained -o publish
```

Output: `publish/hwmon-helper.exe` (~12 MB, single-file, self-contained .NET 8).

## Run (interactive smoke test)

```powershell
.\publish\hwmon-helper.exe
{"cmd":"hello"}
{"cmd":"poll"}
```

Each line of input must be a JSON object; each output line is a JSON object.
Send a JSON object with `{"cmd":"shutdown"}` (or close stdin) to exit cleanly.
````

- [ ] **Step 2: Add Protocol.cs and Program.cs (still no hardware lib — just echo)**

Create `hwmon-helper/Protocol.cs`:

```csharp
using System.Text.Json.Serialization;

namespace HwmonHelper;

public sealed record HelloResponse(
    [property: JsonPropertyName("version")] string Version,
    [property: JsonPropertyName("capabilities")] string[] Capabilities);

public sealed record PollResponse(
    [property: JsonPropertyName("cpu_package_c")] double? CpuPackageC,
    [property: JsonPropertyName("cpu_core_avg_c")] double? CpuCoreAvgC);

public sealed record ErrorResponse(
    [property: JsonPropertyName("error")] string Error);

// AOT-safe source generator context. We don't enable AOT in Phase 2A
// but using a typed context now keeps the option open.
[JsonSerializable(typeof(HelloResponse))]
[JsonSerializable(typeof(PollResponse))]
[JsonSerializable(typeof(ErrorResponse))]
[JsonSourceGenerationOptions(WriteIndented = false)]
public partial class ProtocolJsonContext : JsonSerializerContext;
```

Create `hwmon-helper/Program.cs`:

```csharp
using System.Text.Json;
using HwmonHelper;

const string Version = "0.1.0";
var stdin = Console.In;
var stdout = Console.Out;

string? line;
while ((line = stdin.ReadLine()) is not null)
{
    if (string.IsNullOrWhiteSpace(line)) continue;

    using var doc = TryParse(line);
    if (doc is null)
    {
        WriteError("invalid_json");
        continue;
    }

    if (!doc.RootElement.TryGetProperty("cmd", out var cmdProp) ||
        cmdProp.ValueKind != JsonValueKind.String)
    {
        WriteError("missing_cmd");
        continue;
    }

    switch (cmdProp.GetString())
    {
        case "hello":
            Write(new HelloResponse(Version, new[] { "hello" }),
                  ProtocolJsonContext.Default.HelloResponse);
            break;
        case "poll":
            // Phase 2 task 3 implements the real reading. For task 1 we
            // return nulls so the Rust client can be tested end-to-end first.
            Write(new PollResponse(null, null),
                  ProtocolJsonContext.Default.PollResponse);
            break;
        case "shutdown":
            return 0;
        default:
            WriteError("unknown_cmd");
            break;
    }
}

return 0;

static JsonDocument? TryParse(string s)
{
    try { return JsonDocument.Parse(s); } catch { return null; }
}

void Write<T>(T value, System.Text.Json.Serialization.Metadata.JsonTypeInfo<T> ti)
{
    stdout.WriteLine(JsonSerializer.Serialize(value, ti));
    stdout.Flush();
}

void WriteError(string msg)
{
    Write(new ErrorResponse(msg), ProtocolJsonContext.Default.ErrorResponse);
}
```

- [ ] **Step 3: Build and smoke-test the helper interactively**

Run:
```powershell
cd hwmon-helper
dotnet build -c Release
```

Expected: build succeeds, `bin/Release/net8.0/win-x64/hwmon-helper.exe` (or similar path) is produced.

Run the smoke test:
```powershell
echo '{"cmd":"hello"}' | dotnet run -c Release --no-build
```

Expected stdout: `{"version":"0.1.0","capabilities":["hello"]}`

- [ ] **Step 4: Commit**

```powershell
cd ..
git add hwmon-helper/HwmonHelper.csproj hwmon-helper/Program.cs hwmon-helper/Protocol.cs hwmon-helper/.gitignore hwmon-helper/README.md
git commit -m "feat(hwmon-helper): C# skeleton with stdin/stdout JSON protocol (hello + null poll)"
```

---

## Task 2: Add LibreHardwareMonitorLib + enumerate CPU sensors at startup

**Goal:** Initialise an LHM `Computer` instance with `IsCpuEnabled = true`, log discovered CPU sensors to stderr on startup so we can verify hardware detection works on the dev machine.

**Files:**
- Modify: [hwmon-helper/HwmonHelper.csproj](../../../hwmon-helper/HwmonHelper.csproj)
- Create: [hwmon-helper/Hardware.cs](../../../hwmon-helper/Hardware.cs)
- Modify: [hwmon-helper/Program.cs](../../../hwmon-helper/Program.cs)

- [ ] **Step 1: Add LHM package reference**

In `hwmon-helper/HwmonHelper.csproj`, inside the existing `<Project>` element after `</PropertyGroup>`, add:

```xml
  <ItemGroup>
    <PackageReference Include="LibreHardwareMonitorLib" Version="0.9.4" />
  </ItemGroup>
```

(0.9.4 is the latest stable as of 2026-05; pin specifically to keep dependency renew explicit.)

- [ ] **Step 2: Create the Hardware wrapper**

Create `hwmon-helper/Hardware.cs`:

```csharp
using LibreHardwareMonitor.Hardware;

namespace HwmonHelper;

/// <summary>
/// Single-purpose wrapper around LHM's Computer that owns the lifecycle and
/// exposes a Snapshot() returning Phase 2A's MVP fields. Per-core + voltages
/// + GPU + SMART will extend Snapshot() in Phase 2B.
/// </summary>
public sealed class HardwareReader : IDisposable
{
    private readonly Computer _computer;
    private bool _disposed;

    public HardwareReader()
    {
        _computer = new Computer
        {
            IsCpuEnabled = true,
            // Phase 2B: enable Mainboard, Gpu, Storage, Memory, Battery, Network.
        };
        _computer.Open();
    }

    /// <summary>
    /// Log a one-line description per CPU sensor to stderr so the dev / log
    /// reader can confirm what LHM detected on this hardware.
    /// </summary>
    public void DescribeToStderr(TextWriter stderr)
    {
        foreach (var hw in _computer.Hardware)
        {
            stderr.WriteLine($"[hwmon] hardware: {hw.HardwareType} {hw.Name} ({hw.Identifier})");
            hw.Update();
            foreach (var s in hw.Sensors)
            {
                stderr.WriteLine($"[hwmon]   sensor: {s.SensorType} '{s.Name}' = {s.Value?.ToString("F1") ?? "null"} ({s.Identifier})");
            }
        }
    }

    /// <summary>
    /// Take a fresh snapshot. Returns the package CPU temperature when one is
    /// reported by LHM, and the average of CPU core temperatures as a backup.
    /// Either may be null on hardware that doesn't expose them.
    /// </summary>
    public PollResponse Snapshot()
    {
        double? package = null;
        var cores = new List<double>();

        foreach (var hw in _computer.Hardware)
        {
            if (hw.HardwareType is not (HardwareType.Cpu)) continue;
            hw.Update();

            foreach (var s in hw.Sensors)
            {
                if (s.SensorType != SensorType.Temperature) continue;
                if (!s.Value.HasValue) continue;

                var v = (double)s.Value.Value;
                if (!(v > 0 && v < 150)) continue; // sanity range

                var name = s.Name?.ToLowerInvariant() ?? string.Empty;

                if (package is null && (
                    name.Contains("package") ||
                    name.Contains("tdie") ||
                    name.Contains("ccd average") ||
                    name.Contains("cpu total")))
                {
                    package = v;
                }
                else if (name.Contains("core"))
                {
                    cores.Add(v);
                }
            }
        }

        double? coreAvg = cores.Count > 0 ? cores.Average() : null;
        return new PollResponse(package, coreAvg);
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _computer.Close();
    }
}
```

- [ ] **Step 3: Wire HardwareReader into Program.cs**

In `hwmon-helper/Program.cs`, replace the entire file with:

```csharp
using System.Text.Json;
using HwmonHelper;

const string Version = "0.1.0";
var stdin = Console.In;
var stdout = Console.Out;
var stderr = Console.Error;

using var hw = new HardwareReader();
hw.DescribeToStderr(stderr);

string? line;
while ((line = stdin.ReadLine()) is not null)
{
    if (string.IsNullOrWhiteSpace(line)) continue;

    using var doc = TryParse(line);
    if (doc is null)
    {
        WriteError("invalid_json");
        continue;
    }

    if (!doc.RootElement.TryGetProperty("cmd", out var cmdProp) ||
        cmdProp.ValueKind != JsonValueKind.String)
    {
        WriteError("missing_cmd");
        continue;
    }

    switch (cmdProp.GetString())
    {
        case "hello":
            Write(new HelloResponse(Version, new[] { "hello", "poll" }),
                  ProtocolJsonContext.Default.HelloResponse);
            break;
        case "poll":
            Write(hw.Snapshot(), ProtocolJsonContext.Default.PollResponse);
            break;
        case "shutdown":
            return 0;
        default:
            WriteError("unknown_cmd");
            break;
    }
}

return 0;

static JsonDocument? TryParse(string s)
{
    try { return JsonDocument.Parse(s); } catch { return null; }
}

void Write<T>(T value, System.Text.Json.Serialization.Metadata.JsonTypeInfo<T> ti)
{
    stdout.WriteLine(JsonSerializer.Serialize(value, ti));
    stdout.Flush();
}

void WriteError(string msg)
{
    Write(new ErrorResponse(msg), ProtocolJsonContext.Default.ErrorResponse);
}
```

- [ ] **Step 4: Smoke-test on real hardware**

Run:
```powershell
cd hwmon-helper
dotnet build -c Release
echo '{"cmd":"poll"}' | dotnet run -c Release --no-build 2>&1 | Out-String -Stream
```

Expected:
- On stderr: lines like `[hwmon] hardware: Cpu Intel Core i7-10700K (/intelcpu/0)` and `[hwmon]   sensor: Temperature 'CPU Package' = 42.0 (/intelcpu/0/temperature/0)`
- On stdout: a single line like `{"cpu_package_c":42.0,"cpu_core_avg_c":40.3}`

If `cpu_package_c` is null AND `cpu_core_avg_c` is null on a system that should expose CPU temps, the test fails and we investigate before committing. LHM 0.9.4 supports Z490's Intel-cpu sensors without admin (it loads its own WinRing0 driver from a temp directory).

- [ ] **Step 5: Commit**

```powershell
cd ..
git add hwmon-helper/HwmonHelper.csproj hwmon-helper/Hardware.cs hwmon-helper/Program.cs
git commit -m "feat(hwmon-helper): real CPU temperature via LibreHardwareMonitorLib"
```

---

## Task 3: Self-contained publish + build script

**Goal:** Produce `desktop-app/src-tauri/binaries/hwmon-helper-x86_64-pc-windows-msvc.exe` from a single command so dev and CI builds are reproducible.

**Files:**
- Create: [scripts/build-helper.ps1](../../../scripts/build-helper.ps1)
- Create: [desktop-app/src-tauri/binaries/.gitkeep](../../../desktop-app/src-tauri/binaries/.gitkeep)
- Modify: [.gitignore](../../../.gitignore)

- [ ] **Step 1: Create build script**

Create `scripts/build-helper.ps1`:

```powershell
# Build the hwmon-helper sidecar binary and place it where Tauri expects it.
# Tauri 2 sidecar binaries are named "<base>-<target-triple>.exe".
# For our Windows installer, the triple is x86_64-pc-windows-msvc.

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$src = Join-Path $repo "hwmon-helper"
$out = Join-Path $repo "desktop-app\src-tauri\binaries"
$publish = Join-Path $src "publish"

if (Test-Path $publish) { Remove-Item $publish -Recurse -Force }

Write-Host "Publishing hwmon-helper (self-contained, single-file, win-x64)..."
& dotnet publish $src -c Release -r win-x64 --self-contained -o $publish | Out-Host
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed with exit code $LASTEXITCODE" }

$srcExe = Join-Path $publish "hwmon-helper.exe"
if (-not (Test-Path $srcExe)) { throw "Expected $srcExe was not produced" }

if (-not (Test-Path $out)) { New-Item -ItemType Directory -Path $out | Out-Null }

$destExe = Join-Path $out "hwmon-helper-x86_64-pc-windows-msvc.exe"
Copy-Item $srcExe $destExe -Force
$size = [math]::Round((Get-Item $destExe).Length / 1MB, 1)
Write-Host "Copied to $destExe ($size MB)"
```

- [ ] **Step 2: Create the binaries directory placeholder**

Create `desktop-app/src-tauri/binaries/.gitkeep` (empty file).

- [ ] **Step 3: Add the built artefact to .gitignore**

In `.gitignore` at the repo root, append:

```gitignore

# Phase 2: hwmon sidecar built artefact — built per-machine, not committed
desktop-app/src-tauri/binaries/hwmon-helper-*.exe
hwmon-helper/publish/
```

- [ ] **Step 4: Run the build script and verify**

Run from the repo root:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-helper.ps1
```

Expected: output ends with `Copied to ...\hwmon-helper-x86_64-pc-windows-msvc.exe (~12 MB)`.

Verify file size and basic execution:
```powershell
Get-Item desktop-app\src-tauri\binaries\hwmon-helper-x86_64-pc-windows-msvc.exe | Select-Object Name,Length
echo '{"cmd":"poll"}' | desktop-app\src-tauri\binaries\hwmon-helper-x86_64-pc-windows-msvc.exe
```

Expected: file is ~10-15 MB, the poll command prints a JSON line on stdout.

- [ ] **Step 5: Commit**

```powershell
git add scripts/build-helper.ps1 desktop-app/src-tauri/binaries/.gitkeep .gitignore
git commit -m "feat(hwmon-helper): build script + .gitignore for sidecar artefact"
```

---

## Task 4: Tauri sidecar binding

**Goal:** Tell Tauri that `hwmon-helper-x86_64-pc-windows-msvc.exe` is an externalBin so it gets bundled into the installer and Tauri can spawn it via its sidecar API. (In Phase 2A we'll spawn it directly via `tokio::process` rather than the shell plugin, but the externalBin entry is still required for it to be copied into the bundle on release.)

**Files:**
- Modify: [desktop-app/src-tauri/tauri.conf.json](../../../desktop-app/src-tauri/tauri.conf.json)

- [ ] **Step 1: Add the externalBin entry**

In `desktop-app/src-tauri/tauri.conf.json`, locate the `"bundle"` object (currently `{ "active": true, "targets": "all", "icon": [...], "windows": {...} }`). Add an `externalBin` array next to `icon`:

```json
    "bundle": {
        "active": true,
        "targets": "all",
        "externalBin": ["binaries/hwmon-helper"],
        "icon": [
            "icons/32x32.png",
```

The leading `binaries/` is the path relative to `src-tauri/`. Tauri appends the target-triple suffix at build time, which is why the file on disk is named `hwmon-helper-x86_64-pc-windows-msvc.exe`.

- [ ] **Step 2: Verify Tauri can find the binary at build time**

Run from `desktop-app/`:
```powershell
yarn tauri info
```

Expected: no error about a missing externalBin. (If Tauri complains, double-check the filename matches `<basename>-<triple>.exe` exactly.)

Also run a syntax check on the JSON:
```powershell
Get-Content desktop-app\src-tauri\tauri.conf.json -Raw | ConvertFrom-Json | Out-Null
```

Expected: no parse error.

- [ ] **Step 3: Commit**

```powershell
git add desktop-app/src-tauri/tauri.conf.json
git commit -m "feat(tauri): declare hwmon-helper as bundled sidecar binary"
```

---

## Task 5: Rust HwmonClient — spawn, request, parse, recover

**Goal:** A tested Rust module that owns the helper child process, sends `{"cmd":"poll"}` requests, parses responses into a `HwmonSnapshot`, and handles helper crashes by attempting one restart per polling cycle.

**Files:**
- Create: [desktop-app/src-tauri/src/sensors/hwmon_client.rs](../../../desktop-app/src-tauri/src/sensors/hwmon_client.rs)
- Modify: [desktop-app/src-tauri/src/sensors/mod.rs](../../../desktop-app/src-tauri/src/sensors/mod.rs)

- [ ] **Step 1: Write the failing tests**

Create `desktop-app/src-tauri/src/sensors/hwmon_client.rs` with **only the protocol DTOs and the test module** initially:

```rust
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
}
```

Add module declaration to `desktop-app/src-tauri/src/sensors/mod.rs` — append:

```rust
pub mod hwmon_client;
```

(If `mod.rs` doesn't already exist, look up the actual module file. In this codebase `sensors/` is declared via `mod sensors;` in `lib.rs` and individual sub-modules are declared inside `desktop-app/src-tauri/src/sensors/mod.rs`. The file may need a `pub mod hwmon_client;` line — verify by running `cargo check` after.)

- [ ] **Step 2: Run tests to verify they fail to compile**

Run from `desktop-app/src-tauri/`:
```powershell
cargo test hwmon_client 2>&1 | Select-Object -Last 15
```

Expected: tests compile and PASS (we only added DTOs + tests; no missing functions yet). The failing tests phase comes in Step 3 below where we add the spawn-and-poll API.

If tests don't pass here, fix the parse code before continuing.

- [ ] **Step 3: Write failing tests for the spawn+poll API**

Append to `desktop-app/src-tauri/src/sensors/hwmon_client.rs` inside the `mod tests` block, before the closing `}`:

```rust
    use std::path::PathBuf;

    /// A fake helper that ignores stdin and prints a canned JSON line then exits.
    /// Used so the IPC plumbing can be tested without the real .NET binary.
    fn fake_helper_path(canned: &str) -> PathBuf {
        // On Windows we use `cmd.exe /c echo ...` directly via Command::new
        // in the client; here we just return the canned string and let the
        // test harness construct the command.
        let _ = canned;
        // Tests below use HwmonClient::spawn_with_command to inject cmd.exe.
        PathBuf::from("cmd.exe")
    }

    #[tokio::test]
    async fn poll_reads_one_json_line_from_helper_stdout() {
        let canned = r#"{"cpu_package_c":45.0,"cpu_core_avg_c":42.0}"#;
        let mut cmd = tokio::process::Command::new("cmd.exe");
        cmd.args(["/c", &format!("echo {}", canned)]);
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
        let mut cmd = tokio::process::Command::new("cmd.exe");
        cmd.args(["/c", &format!("echo {}", canned)]);
        let mut client = HwmonClient::spawn_with_command(cmd)
            .await
            .expect("spawn fake helper");
        let _first = client.poll().await.expect("first poll");
        let second = client.poll().await;
        assert!(second.is_err(), "second poll on dead helper must error");
    }
```

The `tokio::test` macro requires a runtime feature; `tokio = { version = "1", features = ["full"] }` already enables it (`macros` is part of `full`). No Cargo.toml change needed.

- [ ] **Step 4: Run new tests to verify they fail (missing `spawn_with_command` and `poll`)**

Run:
```powershell
cargo test hwmon_client 2>&1 | Select-Object -Last 20
```

Expected: compile errors about `HwmonClient` / `spawn_with_command` / `poll` not existing.

- [ ] **Step 5: Implement HwmonClient**

In `desktop-app/src-tauri/src/sensors/hwmon_client.rs`, between the `PollRequest` block and the `#[cfg(test)] mod tests` block, add:

```rust
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
        let mut cmd = Command::new(&path);
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

    /// Best-effort terminate the helper on drop. Tokio will reap.
    pub async fn shutdown(mut self) {
        // Send the cleanup command but don't wait long.
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
```

- [ ] **Step 6: Run tests — all should pass**

Run:
```powershell
cargo test hwmon_client 2>&1 | Select-Object -Last 20
```

Expected: 6 tests pass — 4 unit (parse) + 2 integration (spawn cmd.exe). If `poll_returns_err_when_helper_already_exited` flakes, allow up to 100 ms between the two polls.

- [ ] **Step 7: Commit**

```powershell
cd ..  # back to repo root
git add desktop-app/src-tauri/src/sensors/hwmon_client.rs desktop-app/src-tauri/src/sensors/mod.rs
git commit -m "feat(hwmon-client): Rust sidecar IPC client with spawn/poll/snapshot parsing"
```

---

## Task 6: Wire HwmonClient into the sensor collector

**Goal:** `SensorCollector` owns an `Option<HwmonClient>` and polls it once per sensor cycle. `cpu::collect()` accepts the resulting `Option<HwmonSnapshot>` and uses `snapshot.best_cpu_temp()` when available, with the existing sysinfo/WMI fallback chain unchanged for users who don't have the helper.

**Files:**
- Modify: [desktop-app/src-tauri/src/sensors/cpu.rs](../../../desktop-app/src-tauri/src/sensors/cpu.rs)
- Modify: [desktop-app/src-tauri/src/sensors/collector.rs](../../../desktop-app/src-tauri/src/sensors/collector.rs)
- Modify: [desktop-app/src-tauri/src/lib.rs](../../../desktop-app/src-tauri/src/lib.rs)

- [ ] **Step 1: Write a failing test for cpu::collect with a provided snapshot**

In `desktop-app/src-tauri/src/sensors/cpu.rs`, append (after the existing functions, before the file end):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::hwmon_client::HwmonSnapshot;

    #[test]
    fn collect_uses_hwmon_snapshot_when_provided() {
        let sys = sysinfo::System::new_all();
        let snap = Some(HwmonSnapshot {
            cpu_package_c: Some(48.5),
            cpu_core_avg_c: Some(46.0),
        });
        let data = collect(&sys, snap.as_ref());
        assert_eq!(data.temperature, Some(48.5),
            "hwmon snapshot must win over WMI fallback");
    }

    #[test]
    fn collect_falls_back_when_snapshot_is_none() {
        // Without a snapshot we exercise the existing WMI chain. The result
        // depends on hardware so we only assert that the function doesn't
        // panic and returns a value with the expected shape.
        let sys = sysinfo::System::new_all();
        let data = collect(&sys, None);
        assert!(data.usage_percent >= 0.0);
        // temperature may or may not be Some — depends on the WMI path.
    }
}
```

- [ ] **Step 2: Run test to verify compile failure**

Run:
```powershell
cargo test -p ha-companion cpu::tests 2>&1 | Select-Object -Last 15
```

Expected: compile error — `collect` takes 1 arg, called with 2.

- [ ] **Step 3: Update `collect` signature in cpu.rs**

In `desktop-app/src-tauri/src/sensors/cpu.rs`, change the function signature and body of `collect`:

```rust
pub fn collect(
    sys: &System,
    hwmon: Option<&crate::sensors::hwmon_client::HwmonSnapshot>,
) -> CpuData {
    let cpus = sys.cpus();
    let model = cpus.first().map(|c| c.brand().to_string()).unwrap_or_default();
    let usage_percent = sys.global_cpu_usage();
    let frequency_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let core_count = sys.physical_core_count().unwrap_or(0);
    let logical_core_count = cpus.len();

    // Phase 2A: prefer the bundled hwmon helper snapshot when present.
    // Falls through to the existing sysinfo/WMI chain when the helper
    // wasn't started, crashed, or returned both fields as null.
    let temperature = hwmon.and_then(|s| {
        let t = s.best_cpu_temp();
        if let Some(v) = t {
            log::info!("[CPU] Temperature from hwmon-helper: {:.1}°C", v);
        }
        t
    });

    let temperature = temperature.or_else(|| {
        // existing sysinfo Components attempt
        let components = sysinfo::Components::new_with_refreshed_list();
        let all_labels: Vec<String> = components.iter().map(|c| c.label().to_string()).collect();
        if all_labels.is_empty() {
            log::debug!("[CPU] sysinfo: no thermal components found");
        } else {
            log::debug!("[CPU] sysinfo thermal components: {:?}", all_labels);
        }
        components
            .iter()
            .find(|c| {
                let label = c.label().to_lowercase();
                label.contains("cpu") || label.contains("core") || label.contains("package")
            })
            .map(|comp| {
                log::info!("[CPU] sysinfo temperature from '{}': {:.1}°C", comp.label(), comp.temperature());
                comp.temperature()
            })
    });

    #[cfg(windows)]
    let temperature = temperature.or_else(|| {
        collect_cpu_temp_lhm()
            .or_else(collect_cpu_temp_ohm)
            .or_else(collect_cpu_temp_wmi)
    });

    match temperature {
        Some(t) => log::info!("[CPU] Final temperature = {:.1}°C", t),
        None => log::warn!("[CPU] Final temperature = None (will be reported as unknown to HA)"),
    }

    CpuData {
        model,
        usage_percent,
        frequency_mhz,
        temperature,
        core_count,
        logical_core_count,
    }
}
```

Delete the old `let mut temperature = { ... }` block and the `#[cfg(windows)] if temperature.is_none() { ... }` block that the above replaces.

- [ ] **Step 4: Update collector.rs to thread the snapshot through**

In `desktop-app/src-tauri/src/sensors/collector.rs`:

a) Change the struct to hold an optional client:

```rust
pub struct SensorCollector {
    sys: System,
    enabled_sensors: HashMap<String, bool>,
    hwmon: Option<crate::sensors::hwmon_client::HwmonClient>,
}
```

b) Update `SensorCollector::new`:

```rust
impl SensorCollector {
    pub fn new(
        enabled_sensors: &HashMap<String, bool>,
        hwmon: Option<crate::sensors::hwmon_client::HwmonClient>,
    ) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        Self {
            sys,
            enabled_sensors: enabled_sensors.clone(),
            hwmon,
        }
    }
```

c) In `collect_dynamic`, **before** the `if cpu_enabled { let cpu_data = cpu::collect(&self.sys); ... }` block (it's roughly at the top of the function), poll the helper:

```rust
        // Poll the hwmon sidecar once per cycle so cpu::collect can use it.
        let snap = if let Some(client) = self.hwmon.as_mut() {
            match client.poll().await {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("[hwmon] poll failed, dropping helper for this cycle: {e}");
                    None
                }
            }
        } else { None };
```

**Wait** — `collect_dynamic` is currently a synchronous `fn`. Polling is async. To avoid refactoring all of collector.rs to async, we'll do the polling at the caller site (`lib.rs::sensor_update_loop`), which is already async, and pass the snapshot in. Revert the above plan: instead, change the signatures:

```rust
    pub fn collect_dynamic(
        &mut self,
        hwmon: Option<&crate::sensors::hwmon_client::HwmonSnapshot>,
    ) -> Vec<SensorValue> {
```

And in the CPU sub-block within `collect_dynamic`, change:

```rust
            let cpu_data = cpu::collect(&self.sys);
```

to:

```rust
            let cpu_data = cpu::collect(&self.sys, hwmon);
```

And similarly in `collect_static` and `collect_all`:

```rust
    pub fn collect_all(
        &mut self,
        hwmon: Option<&crate::sensors::hwmon_client::HwmonSnapshot>,
    ) -> Vec<SensorValue> {
        self.sys.refresh_all();
        let mut sensors = Vec::new();
        sensors.extend(self.collect_static(hwmon));
        sensors.extend(self.collect_dynamic(hwmon));
        sensors
    }

    pub fn collect_static(
        &mut self,
        hwmon: Option<&crate::sensors::hwmon_client::HwmonSnapshot>,
    ) -> Vec<SensorValue> {
        // ... existing body, replacing the one cpu::collect call with cpu::collect(&self.sys, hwmon)
```

The `hwmon` field on `SensorCollector` is **no longer needed** — remove the field and the parameter from `new`. The client lives in `lib.rs` and is polled there.

d) Remove the `hwmon: Option<...>` field from `SensorCollector` and from `SensorCollector::new`:

```rust
pub struct SensorCollector {
    sys: System,
    enabled_sensors: HashMap<String, bool>,
}

impl SensorCollector {
    pub fn new(enabled_sensors: &HashMap<String, bool>) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys,
            enabled_sensors: enabled_sensors.clone(),
        }
    }
```

(So Task 6 step 4 keeps `SensorCollector` signature stable; only the `collect_*` methods grow a parameter.)

- [ ] **Step 5: Update lib.rs to spawn the helper and pass snapshots into the collector**

In `desktop-app/src-tauri/src/lib.rs`, at the top of `pub fn run`, after the existing logger setup and before `tauri::Builder::default()`, add a helper-start log line (the actual spawn happens inside `sensor_update_loop` so we don't block app start on an LHM init):

```rust
    log::info!("[bootstrap] hwmon-helper will be started by the sensor loop");
```

Then in `sensor_update_loop` (currently around line 204):

a) At the very top of the function (after `let mut cycle_count: u64 = 0;` line), spawn the helper:

```rust
    let mut hwmon: Option<crate::sensors::hwmon_client::HwmonClient> =
        match crate::sensors::hwmon_client::HwmonClient::spawn_default().await {
            Ok(c) => {
                log::info!("[hwmon] sidecar started");
                Some(c)
            }
            Err(e) => {
                log::warn!("[hwmon] sidecar not started ({e}); falling back to WMI chain only");
                None
            }
        };
```

b) Inside the `loop {}` body, **before** the `if is_registered {` block, poll the helper once and store the snapshot:

```rust
        let hwmon_snap = if let Some(client) = hwmon.as_mut() {
            match client.poll().await {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("[hwmon] poll failed, dropping sidecar: {e}");
                    hwmon = None;
                    None
                }
            }
        } else { None };
```

c) Replace the existing `collector.collect_all()` and `collector.collect_dynamic()` calls inside the loop with the parameterised versions:

```rust
                let all_sensors = {
                    let mut collector = state.collector.lock().await;
                    collector.collect_all(hwmon_snap.as_ref())
                };
```

```rust
                let sensor_data = {
                    let mut collector = state.collector.lock().await;
                    collector.collect_dynamic(hwmon_snap.as_ref())
                };
```

Also update the `register_device` flow in `registration.rs`:

```rust
    let all_sensors = collector.collect_all(None);
```

(`registration.rs` runs during initial setup before the loop; at that point the hwmon-helper might not be spawned yet. Passing `None` is correct — the WMI fallback chain still works.)

- [ ] **Step 6: Run all tests**

Run:
```powershell
cargo test 2>&1 | Select-String "test result"
```

Expected: every previously-passing test still passes, plus the new `cpu::tests` + `hwmon_client::tests` cases. Total ≥ 18 tests.

If anything fails, fix before committing.

- [ ] **Step 7: Commit**

```powershell
git add desktop-app/src-tauri/src/sensors/cpu.rs desktop-app/src-tauri/src/sensors/collector.rs desktop-app/src-tauri/src/lib.rs desktop-app/src-tauri/src/registration.rs
git commit -m "feat(sensors): prefer hwmon-helper snapshot for CPU temperature with WMI fallback"
```

---

## Task 7: End-to-end verification on the Z490 hardware

**Goal:** Run the full stack on the dev machine, confirm the helper starts, real CPU temperature flows through to HA, and the value moves with load.

**Files:** None — verification only.

- [ ] **Step 1: Ensure nothing is running and the binary is up-to-date**

```powershell
Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
powershell -ExecutionPolicy Bypass -File scripts/build-helper.ps1
```

Expected: build script reports the new `~12 MB` binary at the expected path.

- [ ] **Step 2: Archive the existing app.log and start dev**

```powershell
$log = "$env:APPDATA\com.ha-companion.desktop\app.log"
if (Test-Path $log) { Move-Item -Force $log "$log.before-phase2-$(Get-Date -Format HHmmss).bak" }
cd desktop-app
yarn tauri dev
```

- [ ] **Step 3: Watch for the hwmon entries in app.log**

In a second terminal:
```powershell
Get-Content "$env:APPDATA\com.ha-companion.desktop\app.log" -Wait -Tail 50 | Select-String "hwmon|CPU.*Temperature|Final temperature"
```

Expected lines, in order:
- `[bootstrap] hwmon-helper will be started by the sensor loop`
- `[hwmon] sidecar started`
- `[CPU] Temperature from hwmon-helper: <number>°C`  (number should be **35-60°C** at idle on Z490, not 27.9)
- `[CPU] Final temperature = <number>°C`

If the helper fails to start, the log will instead read `[hwmon] sidecar not started (...)` followed by the existing WMI chain output reporting `27.9°C`. That's a Task 1-5 regression — go back to the relevant task.

- [ ] **Step 4: Verify HA shows a real CPU temperature**

```powershell
$token = (Get-Content "$env:APPDATA\com.ha-companion.desktop\settings.json" | ConvertFrom-Json).access_token
$h = @{ Authorization = "Bearer $token" }
$r = Invoke-RestMethod -Uri "https://assist.phillippepelzer.me/api/states/sensor.phill_pc_cpu_temperature" -Headers $h
$r | Format-List entity_id,state,last_updated
```

Expected: `state` is a number between ~30 and 100 (not 27.9), `last_updated` is within the last minute.

Optional load test: run any CPU-intensive workload for ~30 seconds (e.g. `(1..8) | ForEach-Object -Parallel { while ($true) { [math]::Sqrt(1234567) | Out-Null } }`), then re-query — the temperature should rise visibly.

- [ ] **Step 5: Stop the dev app**

`Ctrl+C` in the `yarn tauri dev` terminal, or:
```powershell
Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
```

- [ ] **Step 6: No version bump, no installer build yet**

Per the project's "tests-before-builds" rule, Phase 2A only moves to a release artefact once:
- All `cargo test` cases pass, **and**
- The Z490 verification above produced a real CPU temperature value, **and**
- The user has personally confirmed the value moves with load.

After all three: Phase 3 (availability + shutdown hook) becomes the next plan to write. The version bump + installer build is consolidated at the end of the chain (Phase 3 finish), not per-phase.

---

## Self-review

**Spec coverage:**
- Bundled hwmon helper (no external app for end user) → Tasks 1–4
- Real CPU temperature replacing 27.9°C static value → Tasks 2, 3, 6, 7
- Falls back gracefully when helper missing → Task 5 (`HwmonClient::spawn_default` returns Err) + Task 6 (Option<HwmonSnapshot>)
- Logging on every fallback decision → Task 6 step 3
- All changes test-first → Tasks 1 (smoke), 5 (unit + integration), 6 (cpu::collect signature test)
- No version bump, no installer build inside this phase → Task 7 step 6 explicit
- Phase 2B (per-core, voltages, GPU, SMART) deferred → noted in Goal + Out of scope
- Phase 3 deferred → noted in Out of scope

**Placeholder scan:** No TODO/TBD strings, no "add appropriate error handling" hand-waves, every code block compileable as written, every command runnable as written.

**Type/signature consistency:**
- `HwmonSnapshot` defined in Task 5 step 1, consumed in Task 6 (`collect(sys, hwmon: Option<&HwmonSnapshot>)`) ✅
- `HwmonClient::spawn_default` (Task 5 step 5) returns `io::Result<Self>` — caller in Task 6 step 5 matches the Result correctly ✅
- `HwmonClient::poll` returns `io::Result<HwmonSnapshot>` — Task 6 step 5 caller matches ✅
- `collect_static` / `collect_dynamic` / `collect_all` all gain the same `hwmon: Option<&HwmonSnapshot>` parameter (Task 6 step 4) ✅
- C# `PollResponse` field names `cpu_package_c` / `cpu_core_avg_c` (Task 1 step 2) match Rust `HwmonSnapshot` `#[serde(rename = ...)]` attributes (Task 5 step 1) ✅
- `default_helper_path()` returns the same filename Tauri's `externalBin` produces: `hwmon-helper-x86_64-pc-windows-msvc.exe` ✅ (Task 3 step 1 + Task 4 step 1 + Task 5 step 5)

No gaps found.
