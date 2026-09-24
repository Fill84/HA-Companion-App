const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

if (process.platform !== 'win32') return;

const manifest = path.join(__dirname, 'sensor-service', 'Cargo.toml');
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || null;
const args = ['build', '--release', '--locked', '--manifest-path', manifest];
if (triple) args.push('--target', triple);
const result = spawnSync('cargo', args, {
    stdio: 'inherit',
    shell: false,
});
if (result.status !== 0) throw new Error('Integrated sensor service build failed');

const target = process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(__dirname, 'sensor-service', 'target');
const source = path.join(target, ...(triple ? [triple] : []), 'release', 'ha-companion-sensor-service.exe');
const output = path.join(__dirname, 'src-tauri', 'resources', 'sensor-service');
fs.mkdirSync(output, { recursive: true });
fs.copyFileSync(source, path.join(output, 'ha-companion-sensor-service.exe'));
