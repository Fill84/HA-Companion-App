# Reproduce the signing property assignment without writing configuration or
# importing any certificate. Run with PowerShell 7, as the workflow does.
$ErrorActionPreference = 'Stop'
$auditRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$auditConfigPath = Join-Path $auditRoot 'desktop-app/src-tauri/tauri.conf.json'
$auditConfig = Get-Content -LiteralPath $auditConfigPath -Raw | ConvertFrom-Json
$auditFailure = $null
try {
    $auditConfig.bundle.windows.certificateThumbprint = 'SYNTHETIC-AUDIT-THUMBPRINT'
} catch {
    $auditFailure = $_.Exception.Message
}
[pscustomobject]@{
    reproduced = ($null -ne $auditFailure)
    message = $auditFailure
    wroteConfiguration = $false
} | ConvertTo-Json -Depth 3
if ($null -eq $auditFailure) { exit 1 }
