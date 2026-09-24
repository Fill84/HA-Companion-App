const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function screen({ syncPending = false } = {}) {
    const elements = new Map();
    const calls = [];
    const alerts = [];
    function element() {
        const classes = new Set();
        return {
            value: '', checked: false, children: [], listeners: {},
            classList: {
                add: (...names) => names.forEach(name => classes.add(name)),
                remove: (...names) => names.forEach(name => classes.delete(name)),
                toggle: (name, active) => active ? classes.add(name) : classes.delete(name),
                contains: name => classes.has(name),
            },
            addEventListener(name, fn) { this.listeners[name] = fn; },
            appendChild(child) { this.children.push(child); },
            setAttribute(name, value) { this[name] = value; },
            removeAttribute(name) { delete this[name]; },
            set innerHTML(_value) { this.children = []; },
        };
    }
    const document = {
        addEventListener() {},
        createElement: element,
        getElementById(id) {
            if (!elements.has(id)) elements.set(id, element());
            return elements.get(id);
        },
    };
    const settings = { server_url: 'http://localhost:8123', access_token: 'token',
        update_interval: 60, enabled_sensors: { gpu: true }, is_registered: true };
    const context = vm.createContext({ document, console, t: key => key, setLanguage() {},
        alert: message => { alerts.push(message); },
        window: { __TAURI__: { core: { invoke: async (name, args) => {
            calls.push({ name, args });
            if (name === 'get_settings') return settings;
            if (name === 'get_sensor_list') return [{ id: 'gpu', name: 'GPU', enabled: true, updates_at_interval: true }];
            if (name === 'save_settings') return { sensor_sync_pending: syncPending };
        } } } },
    });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/settings.js'), 'utf8'), context);
    return { context, document, calls, alerts };
}

test('sensor choice is staged until Save and discarded by Cancel', async () => {
    const ui = screen();
    await ui.context.openSettings();
    let checkbox = ui.document.getElementById('sensor-list').children[0].children[0];
    checkbox.checked = false;
    checkbox.listeners.change();
    await ui.context.closeSettings();
    assert.equal(ui.calls.some(call => call.name === 'toggle_sensor' || call.name === 'save_settings'), false);

    await ui.context.openSettings();
    checkbox = ui.document.getElementById('sensor-list').children[0].children[0];
    assert.equal(checkbox.checked, true);
    checkbox.checked = false;
    checkbox.listeners.change();
    await ui.context.saveSettings();
    const saved = ui.calls.find(call => call.name === 'save_settings');
    assert.equal(saved.args.enabledSensors.gpu, false);
});

test('saved preferences with pending HA sync report saved state accurately', async () => {
    const ui = screen({ syncPending: true });
    await ui.context.openSettings();
    await ui.context.saveSettings();
    assert.deepEqual(ui.alerts, ['settings_saved_sync_pending']);
    assert.equal(ui.calls.filter(call => call.name === 'save_settings').length, 1);
});
