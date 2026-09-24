const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

test('remote Home Assistant child webview is outside the local IPC capability', () => {
    const file = path.join(__dirname, '../src-tauri/capabilities/default.json');
    const capability = JSON.parse(fs.readFileSync(file, 'utf8'));
    assert.deepEqual(capability.webviews, ['main']);
    assert.equal(Object.hasOwn(capability, 'windows'), false);
    assert.equal(Object.hasOwn(capability, 'remote'), false);

    const buildSource = fs.readFileSync(path.join(__dirname, '../src-tauri/build.rs'), 'utf8');
    const appSource = fs.readFileSync(path.join(__dirname, '../src-tauri/src/lib.rs'), 'utf8');
    const manifest = buildSource.match(/\.commands\(&\[([\s\S]*?)\]\)/)?.[1];
    const handler = appSource.match(/\.invoke_handler\(tauri::generate_handler!\[([\s\S]*?)\]\)/)?.[1];
    assert.ok(manifest && handler);
    const declared = [...manifest.matchAll(/"([a-z_]+)"/g)].map(match => match[1]).sort();
    const registered = handler.split(',').map(name => name.trim()).filter(Boolean).sort();
    assert.deepEqual(declared, registered);
    for (const command of registered) {
        assert.ok(capability.permissions.includes(`allow-${command.replaceAll('_', '-')}`));
    }
});
