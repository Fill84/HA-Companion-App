param([switch]$ClaimLegacy)
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
    Write-Error 'HA credential owner probe failed; details suppressed.'
    exit 1
} finally {
    if ($socket) { $socket.Dispose() }
    if ($credentialPointer -ne [IntPtr]::Zero) { [CompanionCredentialProbe]::Free($credentialPointer) }
}
