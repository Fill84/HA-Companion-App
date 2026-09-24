param(
    [ValidateRange(5, 3600)]
    [int]$DurationSeconds = 70
)

$ErrorActionPreference = 'Stop'
$filter = "Name='ha-companion.exe' OR Name='ha-companion-sensor-service.exe'"
$before = @(Get-CimInstance Win32_Process -Filter $filter)
if (-not ($before | Where-Object Name -eq 'ha-companion.exe')) {
    throw 'The desktop app is not running.'
}

$start = [DateTime]::UtcNow
Start-Sleep -Seconds $DurationSeconds
$elapsed = ([DateTime]::UtcNow - $start).TotalSeconds
$after = @(Get-CimInstance Win32_Process -Filter $filter)

$samples = foreach ($current in $after) {
    $previous = $before | Where-Object {
        $_.ProcessId -eq $current.ProcessId -and $_.CreationDate -eq $current.CreationDate
    } | Select-Object -First 1
    if (-not $previous) { continue }
    $cpuTicks = ([double]$current.UserModeTime + [double]$current.KernelModeTime) -
        ([double]$previous.UserModeTime + [double]$previous.KernelModeTime)
    [pscustomobject]@{
        Name = $current.Name
        ProcessId = $current.ProcessId
        Seconds = [math]::Round($elapsed, 1)
        CpuPercentOfOneCore = [math]::Round(100 * $cpuTicks / 10000000 / $elapsed, 2)
        WorkingSetMiB = [math]::Round([double]$current.WorkingSetSize / 1048576, 1)
        ReadMiB = [math]::Round(([double]$current.ReadTransferCount - [double]$previous.ReadTransferCount) / 1048576, 2)
        WrittenMiB = [math]::Round(([double]$current.WriteTransferCount - [double]$previous.WriteTransferCount) / 1048576, 2)
    }
}

if (-not ($samples | Where-Object Name -eq 'ha-companion.exe')) {
    throw 'The desktop app restarted during measurement.'
}
$samples | ConvertTo-Json -Compress
