param(
    [ValidateRange(30, 600)]
    [int]$BlockSeconds = 75,
    [ValidateRange(30, 300)]
    [int]$RecoverySeconds = 75
)

$ErrorActionPreference = 'Stop'

$name = 'HACompanionAuditTemporaryHAOutage'
$application = 'C:\Program Files\Home Assistant Companion\ha-companion.exe'
$settings = Join-Path $env:APPDATA 'com.ha-companion.desktop\settings.json'
$settingsHash = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
$ruleCreated = $false

function Start-InteractiveApp {
    $taskName = 'HACompanionAuditOutageLaunch'
    $action = New-ScheduledTaskAction -Execute $application
    $principal = New-ScheduledTaskPrincipal -UserId "$env:COMPUTERNAME\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName $taskName -InputObject (New-ScheduledTask -Action $action -Principal $principal) -Force | Out-Null
    try {
        Start-ScheduledTask -TaskName $taskName
        Start-Sleep -Seconds 7
        $process = Get-Process ha-companion -ErrorAction Stop | Select-Object -First 1
        if ($process.SessionId -ne 1) { throw 'App did not start in the expected interactive session.' }
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
    $recentLog = Get-Content -LiteralPath (Join-Path $env:APPDATA 'com.ha-companion.desktop\app.log') -Tail 40
    $transportError = [bool]($recentLog | Select-String 'Webhook connection failed|Webhook request timed out|Ping failed')
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
