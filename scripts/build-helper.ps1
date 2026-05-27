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
