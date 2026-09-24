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
        } } } },
    });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/settings.js'), 'utf8'), context);
    return { context, document, calls };
}

test('old settings leave the provider off; Save explicitly submits opt-in', async () => {
    const ui = screen({ cpu_temperature_provider_supported: true });
    await ui.context.openSettings();
    const input = ui.document.getElementById('settings-temperature-provider');
    assert.equal(input.checked, false);
    input.checked = true;
    await ui.context.saveSettings();
    assert.equal(ui.calls.find(call => call.name === 'save_settings').args.cpuTemperatureProvider, true);
});

test('Cancel does not save the provider choice; unsupported platforms hide it', async () => {
    const ui = screen({ cpu_temperature_provider_supported: false });
    await ui.context.openSettings();
    assert.equal(ui.document.getElementById('temperature-provider-group').classList.contains('hidden'), true);
    ui.document.getElementById('settings-temperature-provider').checked = true;
    await ui.context.closeSettings();
    assert.equal(ui.calls.some(call => call.name === 'save_settings'), false);
});

test('saved token stays outside the settings view and a blank field keeps it', async () => {
    const ui = screen({
        server_url: 'https://ha.example', has_access_token: true,
        update_interval: 60, language: 'en', enabled_sensors: {},
        cpu_temperature_provider_supported: false,
    });
    await ui.context.openSettings();
    assert.equal(ui.document.getElementById('settings-token').value, '');
    assert.equal(ui.document.getElementById('settings-token').placeholder, 'keep_existing_token');
    await ui.context.saveSettings();
    const save = ui.calls.find(call => call.name === 'save_settings');
    assert.equal(save.args.accessToken, '');
    assert.equal(ui.calls.some(call => call.name === 'register_device'), false);
});
