const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function screen() {
    const elements = new Map();
    let retry = null;
    const calls = [];
    let status = 'ok';
    function element() {
        const classes = new Set(['hidden']);
        return {
            value: '', textContent: '', required: false, placeholder: '',
            classList: {
                add: name => classes.add(name),
                remove: name => classes.delete(name),
                contains: name => classes.has(name),
            },
            setAttribute() {},
        };
    }
    const document = {
        addEventListener() {},
        getElementById(id) {
            if (!elements.has(id)) elements.set(id, element());
            return elements.get(id);
        },
    };
    const window = {
        setInterval(fn) { retry = fn; return 1; },
        clearInterval() { retry = null; },
        __TAURI__: { core: { async invoke(name) {
            calls.push(name);
            if (name === 'check_connection') return status;
        } } },
    };
    const context = vm.createContext({ document, window, console, t: key => key });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/main.js'), 'utf8'), context);
    vm.runInContext('initialSettings = { server_url: "https://ha.example", has_access_token: true }', context);
    document.getElementById('setup-server-url').value = 'https://ha.example';
    return { context, document, calls, retry: () => retry, setStatus: value => { status = value; } };
}

test('temporary startup outage returns to the dashboard without manual Connect', async () => {
    const ui = screen();
    ui.context.showSetupScreen('unreachable');
    assert.ok(ui.retry());
    await ui.retry()();
    assert.deepEqual(ui.calls, ['check_connection', 'load_dashboard']);
    assert.equal(ui.document.getElementById('setup-screen').classList.contains('hidden'), true);
    assert.equal(ui.retry(), null);
});

test('automatic retry preserves edits and stops when the saved token is invalid', async () => {
    const ui = screen();
    ui.context.showSetupScreen('unreachable');
    ui.document.getElementById('setup-token').value = 'new-unsaved-token';
    await ui.retry()();
    assert.deepEqual(ui.calls, []);

    ui.document.getElementById('setup-token').value = '';
    ui.setStatus('token_invalid');
    await ui.retry()();
    assert.deepEqual(ui.calls, ['check_connection']);
    assert.equal(ui.retry(), null);
    assert.equal(ui.document.getElementById('setup-token').required, true);
});
