const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function screen(settings) {
    const elements = new Map();
    const calls = [];
    const document = {
        addEventListener() {},
        getElementById(id) {
            if (!elements.has(id)) {
                const classes = new Set();
                elements.set(id, { value: '', checked: false,
                    classList: {
                        add: (...names) => names.forEach(name => classes.add(name)),
                        remove: (...names) => names.forEach(name => classes.delete(name)),
                        toggle: (name, active) => active ? classes.add(name) : classes.delete(name),
                        contains: name => classes.has(name),
                    },
                });
            }
            return elements.get(id);
        },
    };
    const context = vm.createContext({ document, console, t: key => key, setLanguage() {},
        alert: message => { throw new Error(message); },
        window: { __TAURI__: { core: { invoke: async (name, args) => {
            calls.push({ name, args });
            if (name === 'get_settings') return settings;
            if (name === 'get_sensor_list') return [];
            if (name === 'get_integration_version') return settings.integration_version ?? null;
        } } } },
    });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/settings.js'), 'utf8'), context);
    return { context, document, calls };
}

test('saved token stays outside the settings view and a blank field keeps it', async () => {
    const ui = screen({
        server_url: 'https://ha.example', has_access_token: true,
        update_interval: 60, language: 'en', enabled_sensors: {},
    });
    await ui.context.openSettings();
    assert.equal(ui.document.getElementById('settings-token').value, '');
    assert.equal(ui.document.getElementById('settings-token').placeholder, 'keep_existing_token');
    await ui.context.saveSettings();
    const save = ui.calls.find(call => call.name === 'save_settings');
    assert.equal(save.args.accessToken, '');
    assert.equal(Object.hasOwn(save.args, 'cpuTemperatureProvider'), false);
    assert.equal(ui.calls.some(call => call.name === 'register_device'), false);
});

test('Settings shows the running app version and the connected integration version', async () => {
    const ui = screen({
        app_version: '1.0.6', integration_version: '1.0.11', is_registered: true,
    });
    await ui.context.openSettings();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(ui.document.getElementById('settings-app-version').textContent, 'v1.0.6');
    assert.equal(ui.document.getElementById('settings-integration-version').textContent, 'v1.0.11');
});

test('Older integrations without version metadata show unknown', async () => {
    const ui = screen({ app_version: '1.0.6', is_registered: true });
    await ui.context.openSettings();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(ui.document.getElementById('settings-integration-version').textContent, 'version_unknown');
});
