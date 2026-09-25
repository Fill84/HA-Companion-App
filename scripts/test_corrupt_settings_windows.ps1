param(
    [ValidateRange(1, 100)]
    [int]$ExpectedSessionId = 1,
    [ValidatePattern('^[a-zA-Z0-9._-]+\.json$')]
    [string]$BackupFileName = 'settings-before-corrupt-test.json'
)

$ErrorActionPreference = 'Stop'

$folder = Join-Path $env:APPDATA 'com.ha-companion.desktop'
$settings = Join-Path $folder 'settings.json'
$backup = Join-Path $folder $BackupFileName
$application = 'C:\Program Files\Home Assistant Companion\ha-companion.exe'
if (-not (Test-Path -LiteralPath $backup)) {
    Copy-Item -LiteralPath $settings -Destination $backup
}
$expectedHash = (Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash
if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'Settings changed since the backup; refusing fault injection.'
}

function Start-InteractiveApp([string]$name) {
    $action = New-ScheduledTaskAction -Execute $application
    $principal = New-ScheduledTaskPrincipal -UserId "$env:COMPUTERNAME\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    $task = New-ScheduledTask -Action $action -Principal $principal
    Register-ScheduledTask -TaskName $name -InputObject $task -Force | Out-Null
    try {
        Start-ScheduledTask -TaskName $name
        Start-Sleep -Seconds 7
        $process = Get-Process ha-companion -ErrorAction Stop | Select-Object -First 1
        if ($process.SessionId -ne $ExpectedSessionId) { throw 'App did not start in the expected interactive session.' }
    } finally {
        Unregister-ScheduledTask -TaskName $name -Confirm:$false
    }
}

$result = $null
Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
try {
    Set-Content -LiteralPath $settings -Value '{broken' -NoNewline -Encoding UTF8
    Start-InteractiveApp 'HACompanionAuditCorruptSettings'
    $corruptPreserved = (Get-Content -LiteralPath $settings -Raw) -eq '{broken'
    $errorLogged = [bool](Get-Content -LiteralPath (Join-Path $folder 'app.log') -Tail 30 | Select-String 'Settings could not be loaded')
    $result = [pscustomobject]@{CorruptFilePreserved=$corruptPreserved;LoadErrorLogged=$errorLogged}
} finally {
    Get-Process ha-companion -ErrorAction SilentlyContinue | Stop-Process -Force
    Copy-Item -LiteralPath $backup -Destination $settings -Force
    Start-InteractiveApp 'HACompanionAuditRestoreSettings'
}

if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'Original settings were not restored.'
}
$result | ConvertTo-Json -Compress
