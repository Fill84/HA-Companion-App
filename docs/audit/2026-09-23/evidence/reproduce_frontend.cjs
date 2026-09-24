/* Offline probes: execute actual app JS against a minimal DOM/IPC harness.
 * Does not launch Tauri, contact HA, or access real credentials.
 * Run: node docs/audit/2026-09-23/evidence/reproduce_frontend.cjs
 */
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../../..');
const elements = new Map();
const listeners = new Map();
const calls = [];
const output = {};
function element(id) {
    if (!elements.has(id)) {
        const classes = new Set(id === 'setup-screen' ? [] : ['hidden']);
        elements.set(id, {
            value: '', checked: false, textContent: '',
            classList: { add: (...xs) => xs.forEach(x => classes.add(x)),
                remove: (...xs) => xs.forEach(x => classes.delete(x)),
                contains: x => classes.has(x) },
            addEventListener() {},
        });
    }
    return elements.get(id);
}
let rejectDashboard = false;
const context = vm.createContext({
    console: { error() {}, warn() {} }, alert() {},
    document: {
        getElementById: element,
        querySelectorAll: () => [],
        addEventListener: (event, fn) => {
            const handlers = listeners.get(event) || [];
            handlers.push(fn); listeners.set(event, handlers);
        },
    },
    window: { __TAURI__: {
        core: { invoke: async (name, args) => {
            calls.push({ name, args });
            if (name === 'load_dashboard' && rejectDashboard) throw Error('synthetic dashboard failure');
            return undefined;
        } },
        event: { listen() {} },
    } },
});
for (const name of ['i18n.js', 'settings.js', 'main.js']) {
    vm.runInContext(fs.readFileSync(path.join(root, 'desktop-app/src', name), 'utf8'), context);
}
async function run() {
    // Only settings DOMContentLoaded callback; avoid unrelated app bootstrap.
    listeners.get('DOMContentLoaded')[0]();
    listeners.get('keydown')[0]({ key: 'Escape' });
    await Promise.resolve();
    output.escape_without_open_modal = calls.map(call => call.name);
    assert.equal(calls[0].name, 'load_dashboard');
    calls.length = 0;
    element('settings-interval').value = '1';
    await vm.runInContext('saveSettings()', context);
    output.interval_below_html_minimum = calls.find(call => call.name === 'save_settings').args.updateInterval;
    assert.equal(output.interval_below_html_minimum, 1);
    output.missing_translations = vm.runInContext(
        `['swap_usage','bios_vendor','bios_date','system_uptime','process_count','last_boot','logged_in_user','display'].filter(key => t(key) === key)`, context);
    calls.length = 0;
    rejectDashboard = true;
    await vm.runInContext('handleSetup({ preventDefault() {} })', context);
    output.dashboard_failure_after_registration = {
        setup_hidden: element('setup-screen').classList.contains('hidden'),
        error_hidden: element('setup-error').classList.contains('hidden'),
        calls: calls.map(call => call.name),
    };
    assert.equal(output.dashboard_failure_after_registration.setup_hidden, true);
    assert.equal(output.dashboard_failure_after_registration.error_hidden, false);

    const rust = fs.readFileSync(path.join(root, 'desktop-app/src-tauri/src/commands.rs'), 'utf8');
    const raw = rust.match(/let init_script = format!\(\s*r#"([\s\S]*?)"#/)[1];
    const script = raw.replaceAll('{escaped_url}', 'https://ha.example.invalid')
        .replaceAll('{escaped_token}', 'SYNTHETIC-AUDIT-TOKEN')
        .replaceAll('{{', '{').replaceAll('}}', '}');
    const stored = {};
    vm.runInNewContext(script, {
        window: { location: { origin: 'https://untrusted.example.invalid' } },
        location: { origin: 'https://untrusted.example.invalid' },
        localStorage: { setItem: (key, value) => { stored[key] = value; } }, console,
    });
    output.token_injected_on_untrusted_origin = JSON.parse(stored.hassTokens).access_token === 'SYNTHETIC-AUDIT-TOKEN';
    assert.equal(output.token_injected_on_untrusted_origin, true);
    console.log(JSON.stringify(output, null, 2));
}
run().catch(error => { console.error(error); process.exitCode = 1; });
