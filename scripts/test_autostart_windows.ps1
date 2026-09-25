param(
    [ValidateRange(1, 100)]
    [int]$ExpectedSessionId = 1,
    [ValidatePattern('^[a-zA-Z0-9._-]+\.json$')]
    [string]$BackupFileName = 'settings-before-autostart-test.json'
)

$ErrorActionPreference = 'Stop'

$folder = Join-Path $env:APPDATA 'com.ha-companion.desktop'
$settings = Join-Path $folder 'settings.json'
$backup = Join-Path $folder $BackupFileName
$application = 'C:\Program Files\Home Assistant Companion\ha-companion.exe'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runName = 'Home Assistant Companion'

$original = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
if ($original.autostart -ne $true) { throw 'This test expects autostart to be enabled initially.' }
if (Test-Path -LiteralPath $backup) {
    if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -ne
        (Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash) {
        throw 'Autostart test backup exists but differs from the current settings.'
    }
} else {
    Copy-Item -LiteralPath $settings -Destination $backup
}
$originalHash = (Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash

function Start-InteractiveApp([string]$taskName) {
    $action = New-ScheduledTaskAction -Execute $application
    $principal = New-ScheduledTaskPrincipal -UserId "$env:COMPUTERNAME\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    $task = New-ScheduledTask -Action $action -Principal $principal
    Register-ScheduledTask -TaskName $taskName -InputObject $task -Force | Out-Null
    try {
        Start-ScheduledTask -TaskName $taskName
        Start-Sleep -Seconds 8
        $process = Get-Process ha-companion -ErrorAction Stop | Select-Object -First 1
        if ($process.SessionId -ne $ExpectedSessionId) { throw 'App did not start in the interactive session.' }
    } finally {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
}

function Get-RunEntry {
    $properties = Get-ItemProperty -Path $runKey
    $entry = $properties.PSObject.Properties[$runName]
    if ($entry) { return $entry.Value }
    return $null
}

$disabled = $false
try {
    Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
    $original.autostart = $false
    [IO.File]::WriteAllText($settings, ($original | ConvertTo-Json -Depth 30), [Text.UTF8Encoding]::new($false))
    Start-InteractiveApp 'HACompanionAuditAutostartOff'
    $disabled = -not [bool](Get-RunEntry)
} finally {
    Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
    Copy-Item -LiteralPath $backup -Destination $settings -Force
    Start-InteractiveApp 'HACompanionAuditAutostartRestore'
}

$restored = [bool](Get-RunEntry)
$settingsRestored = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -eq $originalHash
[pscustomobject]@{
    DisabledWhenSettingFalse = $disabled
    EnabledAfterRestore = $restored
    SettingsRestored = $settingsRestored
} | ConvertTo-Json -Compress
if (-not ($disabled -and $restored -and $settingsRestored)) {
    throw 'Autostart toggle did not behave as expected.'
}
