param([switch]$ClaimLegacy, [switch]$CheckLogs, [switch]$RedactLogs)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class CompanionCredentialProbe {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct Credential {
        public uint Flags;
        public uint Type;
        public IntPtr TargetName;
        public IntPtr Comment;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist;
        public uint AttributeCount;
        public IntPtr Attributes;
        public IntPtr TargetAlias;
        public IntPtr UserName;
    }

    [DllImport("advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool Read(string target, uint type, uint reserved, out IntPtr credential);

    [DllImport("advapi32.dll", EntryPoint = "CredFree")]
    public static extern void Free(IntPtr credential);
}
'@

function Receive-Message($socket) {
    $buffer = New-Object byte[] 4096
    $stream = [IO.MemoryStream]::new()
    try {
        do {
            $segment = [ArraySegment[byte]]::new($buffer)
            $result = $socket.ReceiveAsync($segment, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
            if ($result.MessageType -ne [Net.WebSockets.WebSocketMessageType]::Text) { throw 'Unexpected socket message' }
            $stream.Write($buffer, 0, $result.Count)
        } until ($result.EndOfMessage)
        return ([Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json)
    } finally {
        $stream.Dispose()
    }
}

function Send-Message($socket, $message) {
    $bytes = [Text.Encoding]::UTF8.GetBytes(($message | ConvertTo-Json -Compress))
    $segment = [ArraySegment[byte]]::new($bytes)
    $null = $socket.SendAsync($segment, [Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
}

$socket = $null
$credentialPointer = [IntPtr]::Zero
$phase = 'read credential'
try {
    $settingsPath = Join-Path $env:APPDATA 'com.ha-companion.desktop\settings.json'
    $settings = Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
    $target = "$($settings.device_id).com.ha-companion.desktop.access-token"
    if (-not [CompanionCredentialProbe]::Read($target, 1, 0, [ref]$credentialPointer)) { throw 'Credential unavailable' }
    $credential = [Runtime.InteropServices.Marshal]::PtrToStructure($credentialPointer, [type][CompanionCredentialProbe+Credential])
    $bytes = New-Object byte[] $credential.CredentialBlobSize
    [Runtime.InteropServices.Marshal]::Copy($credential.CredentialBlob, $bytes, 0, $bytes.Length)
    $stored = [Text.Encoding]::Unicode.GetString($bytes).TrimEnd([char]0) | ConvertFrom-Json
    if ($stored.server_url -ne $settings.server_url -or [string]::IsNullOrEmpty($stored.access_token)) { throw 'Credential does not match settings' }

    if ($CheckLogs -or $RedactLogs) {
        $phase = 'list local logs'
        $folder = Split-Path -Parent $settingsPath
        $logs = @(Get-ChildItem -LiteralPath $folder -Filter 'app.log*' -File)
        $tokenExposed = $false
        $webhookExposed = $false
        $exposedFiles = @()
        $redactedFiles = @()
        $phase = 'read local logs'
        foreach ($logFile in $logs) {
            $content = Get-Content -LiteralPath $logFile.FullName -Raw -ErrorAction Stop
            if ($RedactLogs -and ($content.Contains($stored.access_token) -or
                ($settings.webhook_id -and $content.Contains($settings.webhook_id)))) {
                $phase = 'redact historical log'
                $resolved = [IO.Path]::GetFullPath($logFile.FullName)
                $root = [IO.Path]::GetFullPath($folder).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
                if (-not $resolved.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
                    throw 'Log path is outside the app data directory'
                }
                $sanitized = $content.Replace($stored.access_token, '[redacted]')
                if ($settings.webhook_id) {
                    $sanitized = $sanitized.Replace($settings.webhook_id, '[redacted]')
                }
                $temporary = $resolved + '.redacted-' + [guid]::NewGuid().ToString('N') + '.tmp'
                $phase = 'write sanitized log'
                [IO.File]::WriteAllText($temporary, $sanitized, [Text.UTF8Encoding]::new($false))
                $phase = 'replace original log'
                [IO.File]::Move($temporary, $resolved, $true)
                $redactedFiles += $logFile.Name
                $content = Get-Content -LiteralPath $resolved -Raw -ErrorAction Stop
            }
            $phase = 'compare local log contents'
            $fileTokenExposed = $content.Contains($stored.access_token)
            $fileWebhookExposed = $false
            if ($settings.webhook_id) {
                $fileWebhookExposed = $content.Contains($settings.webhook_id)
            }
            $tokenExposed = $tokenExposed -or $fileTokenExposed
            $webhookExposed = $webhookExposed -or $fileWebhookExposed
            if ($fileTokenExposed -or $fileWebhookExposed) { $exposedFiles += $logFile.Name }
            $phase = 'read local logs'
        }
        $phase = 'report local log check'
        [pscustomobject]@{LogFiles=$logs.Count;TokenExposed=$tokenExposed;WebhookExposed=$webhookExposed;ExposedFiles=$exposedFiles;RedactedFiles=$redactedFiles} | ConvertTo-Json -Compress
        if ($tokenExposed -or $webhookExposed) { exit 2 }
        return
    }

    $phase = 'query Home Assistant'
    $socket = [Net.WebSockets.ClientWebSocket]::new()
    $socket.Options.KeepAliveInterval = [TimeSpan]::FromSeconds(10)
    $uri = [Uri]::new(($settings.server_url -replace '^http', 'ws').TrimEnd('/') + '/api/websocket')
    $null = $socket.ConnectAsync($uri, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    if ((Receive-Message $socket).type -ne 'auth_required') { throw 'Unexpected HA handshake' }
    Send-Message $socket @{type='auth'; access_token=$stored.access_token}
    if ((Receive-Message $socket).type -ne 'auth_ok') { throw 'HA authentication failed' }
    Send-Message $socket @{id=1; type='auth/current_user'}
    $reply = Receive-Message $socket
    if ($reply.type -ne 'result' -or -not $reply.success) { throw 'HA user lookup failed' }
    [pscustomobject]@{ UserId=$reply.result.id; IsAdmin=$reply.result.is_admin; IsOwner=$reply.result.is_owner }
    if ($ClaimLegacy) {
        if (-not $reply.result.is_admin) { throw 'Owner migration requires an HA administrator' }
        $body = @{device_id=$settings.device_id; device_name=$env:COMPUTERNAME} | ConvertTo-Json -Compress
        $response = Invoke-RestMethod -Method Post -Uri ($settings.server_url.TrimEnd('/') + '/api/desktop_app/registrations') -Headers @{Authorization="Bearer $($stored.access_token)"} -ContentType 'application/json' -Body $body
        if (-not $response.success -or $response.webhook_id -ne $settings.webhook_id) { throw 'Owner migration returned an unexpected registration' }
        Write-Output 'Legacy owner migration acknowledged with the existing webhook.'
    }
} catch {
    $inner = $_.Exception.InnerException
    $kind = if ($inner) { $inner.GetType().Name } else { $_.Exception.GetType().Name }
    $parameter = if ($inner -is [ArgumentException]) { $inner.ParamName } else { $null }
    Write-Error "HA credential owner probe failed during $phase ($kind, parameter=$parameter); details suppressed."
    exit 1
} finally {
    if ($socket) { $socket.Dispose() }
    if ($credentialPointer -ne [IntPtr]::Zero) { [CompanionCredentialProbe]::Free($credentialPointer) }
}
