param(
    [ValidateRange(1, 100)]
    [int]$ExpectedSessionId = 1,
    [ValidateRange(60, 180)]
    [int]$TestSeconds = 110,
    [ValidatePattern('^[a-zA-Z0-9._-]+\.json$')]
    [string]$OriginalFileName = 'settings-during-sensor-disable-test.json',
    [ValidatePattern('^[a-zA-Z0-9._-]+\.json$')]
    [string]$DisabledBackupFileName = 'settings-sensor-disable-test-backup.json'
)

$ErrorActionPreference = 'Stop'

$folder = Join-Path $env:APPDATA 'com.ha-companion.desktop'
$settings = Join-Path $folder 'settings.json'
$original = Join-Path $folder $OriginalFileName
$disabledBackup = Join-Path $folder $DisabledBackupFileName
$application = 'C:\Program Files\Home Assistant Companion\ha-companion.exe'

if (Test-Path -LiteralPath $original) { throw 'Temporary settings path already exists.' }
if (Test-Path -LiteralPath $disabledBackup) { throw 'Disabled-profile backup path already exists.' }
$originalHash = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
$config = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
if (($config.enabled_sensors.PSObject.Properties.Name -contains 'cpu_temperature') -and
    $config.enabled_sensors.cpu_temperature -ne $true) {
    throw 'CPU temperature must be explicitly enabled before this test.'
}

function Start-InteractiveApp([string]$taskName) {
    $action = New-ScheduledTaskAction -Execute $application
    $principal = New-ScheduledTaskPrincipal -UserId "$env:COMPUTERNAME\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName $taskName -InputObject (New-ScheduledTask -Action $action -Principal $principal) -Force | Out-Null
    try {
        Start-ScheduledTask -TaskName $taskName
        Start-Sleep -Seconds 8
        $process = Get-Process ha-companion -ErrorAction Stop | Select-Object -First 1
        if ($process.SessionId -ne $ExpectedSessionId) { throw 'App did not start in the interactive session.' }
    } finally {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
}

Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
[IO.File]::Move($settings, $original)
try {
    if ($config.enabled_sensors.PSObject.Properties.Name -contains 'cpu_temperature') {
        $config.enabled_sensors.cpu_temperature = $false
    } else {
        $config.enabled_sensors | Add-Member -NotePropertyName cpu_temperature -NotePropertyValue $false
    }
    $json = $config | ConvertTo-Json -Depth 32
    [IO.File]::WriteAllText($settings, $json, [Text.UTF8Encoding]::new($false))
    Start-InteractiveApp 'HACompanionAuditSensorDisabled'
    Write-Output 'SENSOR_DISABLED_STARTED'
    Start-Sleep -Seconds $TestSeconds
} finally {
    Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
    if (Test-Path -LiteralPath $settings) {
        [IO.File]::Replace($original, $settings, $disabledBackup)
    } else {
        [IO.File]::Move($original, $settings)
    }
    Start-InteractiveApp 'HACompanionAuditSensorRestored'
    $restored = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -eq $originalHash
    Write-Output "RESTORED_ORIGINAL_SETTINGS=$restored"
    if (-not $restored) { throw 'Original settings were not restored byte for byte.' }
}
