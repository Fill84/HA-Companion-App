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
