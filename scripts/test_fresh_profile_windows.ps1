$ErrorActionPreference = 'Stop'

$folder = Join-Path $env:APPDATA 'com.ha-companion.desktop'
$settings = Join-Path $folder 'settings.json'
$temporary = Join-Path $folder 'settings-during-fresh-profile-test.json'
$freshBackup = Join-Path $folder 'settings-generated-during-fresh-profile-test-retry.json'
$application = 'C:\Program Files\Home Assistant Companion\ha-companion.exe'

if (Test-Path -LiteralPath $temporary) { throw 'Temporary settings path already exists.' }
if (Test-Path -LiteralPath $freshBackup) { throw 'Fresh-profile backup path already exists.' }
$originalHash = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
$original = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json

function Start-InteractiveApp([string]$taskName) {
    $action = New-ScheduledTaskAction -Execute $application
    $principal = New-ScheduledTaskPrincipal -UserId "$env:COMPUTERNAME\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName $taskName -InputObject (New-ScheduledTask -Action $action -Principal $principal) -Force | Out-Null
    try {
        Start-ScheduledTask -TaskName $taskName
        Start-Sleep -Seconds 8
        $process = Get-Process ha-companion -ErrorAction Stop | Select-Object -First 1
        if ($process.SessionId -ne 1) { throw 'App did not start in the interactive session.' }
    } finally {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
}

$fresh = $null
Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
[IO.File]::Move($settings, $temporary)
try {
    Start-InteractiveApp 'HACompanionAuditFreshProfile'
    $created = Test-Path -LiteralPath $settings
    if ($created) {
        $freshSettings = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
        $fresh = [pscustomobject]@{
            SettingsCreated = $true
            NewDeviceIdentity = $freshSettings.device_id -ne $original.device_id
            NoServer = [string]::IsNullOrEmpty($freshSettings.server_url)
            NoWebhook = [string]::IsNullOrEmpty($freshSettings.webhook_id)
            NoPlaintextToken = -not [bool]($freshSettings.PSObject.Properties.Name -contains 'access_token')
        }
    } else {
        $fresh = [pscustomobject]@{ SettingsCreated = $false }
    }
} finally {
    Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
    if (Test-Path -LiteralPath $settings) {
        [IO.File]::Replace($temporary, $settings, $freshBackup)
    } else {
        [IO.File]::Move($temporary, $settings)
    }
    Start-InteractiveApp 'HACompanionAuditRestoreProfile'
}

$fresh | Add-Member -NotePropertyName RestoredOriginalSettings -NotePropertyValue ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -eq $originalHash)
$fresh | ConvertTo-Json -Compress
if (-not ($fresh.SettingsCreated -and $fresh.NewDeviceIdentity -and $fresh.NoServer -and
    $fresh.NoWebhook -and $fresh.NoPlaintextToken -and $fresh.RestoredOriginalSettings)) {
    throw 'Fresh-profile startup did not behave as expected.'
}
