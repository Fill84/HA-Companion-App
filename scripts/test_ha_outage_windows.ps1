param(
    [ValidateRange(30, 600)]
    [int]$BlockSeconds = 75,
    [ValidateRange(30, 300)]
    [int]$RecoverySeconds = 75,
    [ValidateRange(1, 100)]
    [int]$ExpectedSessionId = 1
)

$ErrorActionPreference = 'Stop'

$name = 'HACompanionAuditTemporaryHAOutage'
$application = 'C:\Program Files\Home Assistant Companion\ha-companion.exe'
$settings = Join-Path $env:APPDATA 'com.ha-companion.desktop\settings.json'
$settingsHash = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
$logPath = Join-Path $env:APPDATA 'com.ha-companion.desktop\app.log'
$logOffset = if (Test-Path -LiteralPath $logPath) { (Get-Item -LiteralPath $logPath).Length } else { 0 }
$ruleCreated = $false
$transportError = $false
$sameSettings = $false
if (Get-NetFirewallRule -DisplayName $name -ErrorAction SilentlyContinue) {
    throw 'Temporary outage firewall rule already exists; refusing to replace it.'
}

function Start-InteractiveApp {
    $taskName = 'HACompanionAuditOutageLaunch'
    $action = New-ScheduledTaskAction -Execute $application
    $principal = New-ScheduledTaskPrincipal -UserId "$env:COMPUTERNAME\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName $taskName -InputObject (New-ScheduledTask -Action $action -Principal $principal) -Force | Out-Null
    try {
        Start-ScheduledTask -TaskName $taskName
        Start-Sleep -Seconds 7
        $process = Get-Process ha-companion -ErrorAction Stop | Select-Object -First 1
        if ($process.SessionId -ne $ExpectedSessionId) { throw 'App did not start in the expected interactive session.' }
    } finally {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
}

try {
    New-NetFirewallRule -DisplayName $name -Direction Outbound -Action Block -Program $application -Profile Any | Out-Null
    $ruleCreated = $true
    Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-InteractiveApp
    Write-Output 'OUTAGE_ACTIVE'
    Start-Sleep -Seconds $BlockSeconds
    $sameSettings = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -eq $settingsHash
    $stream = [IO.File]::Open($logPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        [void]$stream.Seek($logOffset, [IO.SeekOrigin]::Begin)
        $reader = [IO.StreamReader]::new($stream)
        $newLog = $reader.ReadToEnd()
    } finally {
        $stream.Dispose()
    }
    $transportError = [bool]($newLog -match 'Webhook connection failed|Webhook request timed out|Ping failed')
    [pscustomobject]@{SettingsPreserved=$sameSettings;TransportErrorObserved=$transportError} | ConvertTo-Json -Compress
} finally {
    if ($ruleCreated) { Remove-NetFirewallRule -DisplayName $name -ErrorAction Stop }
    Write-Output 'OUTAGE_REMOVED'
}

Start-Sleep -Seconds $RecoverySeconds
[pscustomobject]@{
    SettingsPreserved=((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -eq $settingsHash)
    AppRunning=([bool](Get-Process ha-companion -ErrorAction SilentlyContinue))
    FirewallRuleGone=(-not [bool](Get-NetFirewallRule -DisplayName $name -ErrorAction SilentlyContinue))
} | ConvertTo-Json -Compress

if (-not ($sameSettings -and $transportError)) {
    throw 'Outage was not proven: settings changed or no new transport error was logged.'
}
