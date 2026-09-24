const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

test('unreadable settings stop setup from replacing the device identity', async () => {
    const elements = new Map();
    const submit = { disabled: false };
    const classes = () => {
        const values = new Set(['hidden']);
        return {
            add: value => values.add(value),
            remove: value => values.delete(value),
            contains: value => values.has(value),
        };
    };
    const document = {
        addEventListener() {},
        querySelector: selector => selector === '#setup-form button[type="submit"]' ? submit : null,
        getElementById(id) {
            if (!elements.has(id)) elements.set(id, { classList: classes(), textContent: '' });
            return elements.get(id);
        },
    };
    const calls = [];
    const context = vm.createContext({
        document,
        console: { error() {} },
        t: key => key,
        window: { __TAURI__: { core: { invoke: async name => {
            calls.push(name);
            throw new Error('settings store damaged');
        } } } },
    });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/main.js'), 'utf8'), context);
    await context.initApp();

    assert.deepEqual(calls, ['get_settings']);
    assert.equal(submit.disabled, true);
    assert.equal(document.getElementById('setup-error').textContent, 'settings_startup_failed');
    assert.equal(document.getElementById('setup-error').classList.contains('hidden'), false);
});
