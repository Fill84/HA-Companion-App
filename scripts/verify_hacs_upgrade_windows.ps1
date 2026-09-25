param(
    [Parameter(Mandatory = $true)][string]$RepositoryId,
    [Parameter(Mandatory = $true)][string]$ExpectedVersion,
    [switch]$Install
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class HacsCredentialReader {
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

function Receive-HaMessage($socket) {
    $buffer = New-Object byte[] 4096
    $stream = [IO.MemoryStream]::new()
    try {
        do {
            $segment = [ArraySegment[byte]]::new($buffer)
            $result = $socket.ReceiveAsync($segment, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
            if ($result.MessageType -ne [Net.WebSockets.WebSocketMessageType]::Text) { throw 'Unexpected HA socket message' }
            $stream.Write($buffer, 0, $result.Count)
        } until ($result.EndOfMessage)
        return ([Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json)
    } finally { $stream.Dispose() }
}

function Send-HaMessage($socket, $message) {
    $bytes = [Text.Encoding]::UTF8.GetBytes(($message | ConvertTo-Json -Compress))
    $segment = [ArraySegment[byte]]::new($bytes)
    $null = $socket.SendAsync($segment, [Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
}

function Invoke-HaCommand($socket, $id, $type, $arguments) {
    $message = @{ id = $id; type = $type }
    foreach ($key in $arguments.Keys) { $message[$key] = $arguments[$key] }
    Send-HaMessage $socket $message
    do { $reply = Receive-HaMessage $socket } until ($reply.type -eq 'result' -and $reply.id -eq $id)
    if (-not $reply.success) { throw "Home Assistant rejected $type" }
    return $reply.result
}

$credentialPointer = [IntPtr]::Zero
$socket = $null
$phase = 'read local credential'
try {
    if ($RepositoryId -notmatch '^\d+$' -or $ExpectedVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid expected repository or version' }
    $settings = Get-Content -LiteralPath (Join-Path $env:APPDATA 'com.ha-companion.desktop\settings.json') -Raw | ConvertFrom-Json
    $target = "$($settings.device_id).com.ha-companion.desktop.access-token"
    if (-not [HacsCredentialReader]::Read($target, 1, 0, [ref]$credentialPointer)) { throw 'Credential unavailable' }
    $credential = [Runtime.InteropServices.Marshal]::PtrToStructure($credentialPointer, [type][HacsCredentialReader+Credential])
    $bytes = New-Object byte[] $credential.CredentialBlobSize
    [Runtime.InteropServices.Marshal]::Copy($credential.CredentialBlob, $bytes, 0, $bytes.Length)
    $stored = [Text.Encoding]::Unicode.GetString($bytes).TrimEnd([char]0) | ConvertFrom-Json
    [Array]::Clear($bytes, 0, $bytes.Length)
    if ($stored.server_url -ne $settings.server_url -or [string]::IsNullOrWhiteSpace($stored.access_token)) { throw 'Credential does not match settings' }

    $phase = 'authenticate to Home Assistant'
    $socket = [Net.WebSockets.ClientWebSocket]::new()
    $uri = [Uri]::new(($settings.server_url -replace '^http', 'ws').TrimEnd('/') + '/api/websocket')
    $null = $socket.ConnectAsync($uri, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    if ((Receive-HaMessage $socket).type -ne 'auth_required') { throw 'Unexpected HA handshake' }
    Send-HaMessage $socket @{ type = 'auth'; access_token = $stored.access_token }
    if ((Receive-HaMessage $socket).type -ne 'auth_ok') { throw 'HA authentication failed' }
    $stored.access_token = $null
    $user = Invoke-HaCommand $socket 1 'auth/current_user' @{}
    if (-not $user.is_admin) { throw 'HACS update requires an HA administrator' }

    $phase = 'refresh HACS repository'
    $null = Invoke-HaCommand $socket 2 'hacs/repository/refresh' @{ repository = $RepositoryId }
    $info = Invoke-HaCommand $socket 3 'hacs/repository/info' @{ repository_id = $RepositoryId }
    if ($info.full_name -ne 'Fill84/ha-integration' -or $info.available_version -ne $ExpectedVersion) { throw 'Unexpected HACS repository or available version' }
    Write-Output "HACS available=$($info.available_version) installed=$($info.installed_version) pending=$($info.pending_upgrade)"

    if ($Install) {
        $phase = 'download HACS integration'
        $null = Invoke-HaCommand $socket 4 'hacs/repository/download' @{ repository = $RepositoryId; version = $ExpectedVersion }
        $info = Invoke-HaCommand $socket 5 'hacs/repository/info' @{ repository_id = $RepositoryId }
        if ($info.installed_version -ne $ExpectedVersion) { throw 'HACS did not record the expected installed version' }
        Write-Output "HACS installed=$($info.installed_version)"
    }
} catch {
    Write-Error "HACS verification failed during $phase; details suppressed."
    exit 1
} finally {
    if ($socket) { $socket.Dispose() }
    if ($credentialPointer -ne [IntPtr]::Zero) { [HacsCredentialReader]::Free($credentialPointer) }
}
