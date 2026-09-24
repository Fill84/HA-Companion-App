param([switch]$Redact)

$ErrorActionPreference = 'Stop'
$resultPath = Join-Path $PSScriptRoot 'ha-companion-log-check-result.txt'
$probe = Join-Path $PSScriptRoot 'verify_ha_credential_owner.ps1'

if ($Redact) {
    & $probe -RedactLogs *> $resultPath
} else {
    & $probe -CheckLogs *> $resultPath
}
