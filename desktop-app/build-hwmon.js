const { spawnSync } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

if (process.platform !== 'win32') process.exit(0);
const target = process.env.TAURI_ENV_TARGET_TRIPLE || `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-pc-windows-msvc`;
const runtime = target === 'x86_64-pc-windows-msvc' ? 'win-x64'
    : target === 'aarch64-pc-windows-msvc' ? 'win-arm64' : null;
if (!runtime) throw new Error(`Unsupported Windows temperature helper target: ${target}`);
const resourceRoot = path.resolve(__dirname, 'src-tauri', 'resources');
const output = path.resolve(resourceRoot, 'hwmon-net10');
if (path.dirname(output) !== resourceRoot) throw new Error('Unsafe helper output path');
const readmePath = path.join(output, 'README.txt');
let readme = null;
if (fs.existsSync(output)) {
    const info = fs.lstatSync(output);
    if (!info.isDirectory() || info.isSymbolicLink()) throw new Error('Unsafe helper output directory');
    if (fs.existsSync(readmePath)) readme = fs.readFileSync(readmePath);
    fs.rmSync(output, { recursive: true });
}
fs.mkdirSync(output, { recursive: true });
if (readme) fs.writeFileSync(readmePath, readme);
const project = path.join(__dirname, '..', 'hwmon-helper', 'HwmonHelper.csproj');
const result = spawnSync('dotnet', ['publish', project, '-c', 'Release', '-r', runtime,
    '--self-contained', 'true', '-p:PublishSingleFile=true',
    '-p:EnableCompressionInSingleFile=true',
    '-p:RestoreLockedMode=true', '-o', output], {
    cwd: path.dirname(project),
    stdio: 'inherit',
});
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status || 1);
if (!fs.existsSync(path.join(output, 'ha-hwmon.exe'))) throw new Error('Missing temperature helper output');
fs.rmSync(path.join(output, 'ha-hwmon.pdb'), { force: true });
for (const name of fs.readdirSync(output)) {
    if (name.toLowerCase().endsWith('.sys')) throw new Error('Kernel driver found in helper output');
}
fs.copyFileSync(path.join(__dirname, '..', 'hwmon-helper', 'THIRD-PARTY-NOTICES.md'),
    path.join(output, 'THIRD-PARTY-NOTICES.md'));
fs.copyFileSync(path.join(__dirname, '..', 'hwmon-helper', 'LICENSE.txt'), path.join(output, 'LICENSE.txt'));
fs.cpSync(path.join(__dirname, '..', 'hwmon-helper', 'licenses'), path.join(output, 'licenses'), { recursive: true });
