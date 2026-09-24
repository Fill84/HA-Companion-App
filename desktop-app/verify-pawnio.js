const { createHash } = require('node:crypto');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, 'src-tauri', 'resources', 'pawnio');
const expected = {
    'x64/PawnIO.sys': 'fca6e7d58b0cf38dbb913a2b9e532f48629145d395f454b16a9f58e97b8d3940',
    'x64/PawnIO.inf': '7c1c203e13693531243fbee3cb87d7b79170eae89f5729b3f41387fe68a54f0b',
    'x64/pawnio.cat': 'a37d46840280efec92063d3a21014c803939e599b4c0da4a4d063b79eeca9446',
    'arm64/PawnIO.sys': '8113d5850e4d7d2cbf7573b12a1de57f254f51b925b017e105fb558ce3a16600',
    'arm64/PawnIO.inf': '9989a2d6985f1131dc7618cb0a4e76f5e59c4837c5057ed765b80bc979ac00cb',
    'arm64/pawnio.cat': 'd95848578d62d33d9eed30b7478d323611818ff6a38c43363aca58777e2f8d35',
    'modules/IntelMSR.bin': 'd6ed85d65ab17a22f813ef98207d6d537155ee2ded5976a21cb48413c9b92e5f',
};

for (const [relative, hash] of Object.entries(expected)) {
    const file = path.join(root, relative);
    const info = fs.lstatSync(file);
    if (!info.isFile() || info.isSymbolicLink()) throw new Error(`Invalid sensor resource: ${relative}`);
    const actual = createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    if (actual !== hash) throw new Error(`Sensor resource hash mismatch: ${relative}`);
}

if (process.platform === 'win32') {
    const script = `
foreach ($arch in @('x64', 'arm64')) {
    $catalog = Join-Path $env:HA_PAWNIO_RESOURCE_ROOT "$arch\\pawnio.cat"
    $signature = Get-AuthenticodeSignature -LiteralPath $catalog
    if ($signature.Status -ne 'Valid' -or
        $signature.SignerCertificate.Subject -notmatch 'Microsoft Windows Hardware Compatibility Publisher') {
        throw "Invalid PawnIO catalog signature: $arch"
    }
}`;
    const result = spawnSync('pwsh.exe', ['-NoProfile', '-NonInteractive', '-Command', script], {
        env: { ...process.env, HA_PAWNIO_RESOURCE_ROOT: root },
        encoding: 'utf8',
    });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(result.stderr || 'PawnIO signature verification failed');
}

console.log('Pinned PawnIO driver and module resources verified');
