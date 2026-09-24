const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
test('every collected sensor choice has English and Dutch labels in the shared catalog', () => {
    const catalog = fs.readFileSync(path.join(__dirname, '../src-tauri/src/sensors/catalog.rs'), 'utf8');
    const choices = [...catalog.matchAll(/SensorChoice\s*\{\s*id:\s*"([a-z_]+)",\s*name_en:\s*"([^"]+)",\s*name_nl:\s*"([^"]+)",\s*updates_at_interval:\s*(true|false)/g)];
    assert.equal(choices.length, 23);
    const ids = choices.map(choice => choice[1]);
    assert.equal(new Set(ids).size, ids.length, 'duplicate sensor choice');
    for (const [, id, english, dutch] of choices) {
        assert.notEqual(english, id);
        assert.notEqual(dutch, id);
    }

    const collector = fs.readFileSync(path.join(__dirname, '../src-tauri/src/sensors/collector.rs'), 'utf8');
    const collected = [...new Set([...collector.matchAll(/self\.is_enabled\("([a-z_]+)"\)/g)].map(match => match[1]))];
    assert.deepEqual([...ids].sort(), collected.sort());

    const settings = fs.readFileSync(path.join(__dirname, '../src/settings.js'), 'utf8');
    const i18n = fs.readFileSync(path.join(__dirname, '../src/i18n.js'), 'utf8');
    assert.match(settings, /data-sensor-nl/);
    assert.match(i18n, /data-sensor-nl/);
});

test('sensor labels follow live language changes without another backend call', () => {
    const values = { 'data-sensor-key': 'cpu_usage', 'data-sensor-en': 'CPU Usage', 'data-sensor-nl': 'CPU Gebruik' };
    const label = { getAttribute: key => values[key], textContent: '' };
    const document = {
        documentElement: { lang: 'en' },
        querySelectorAll: selector => selector === '[data-sensor-key]' ? [label] : [],
    };
    const source = fs.readFileSync(path.join(__dirname, '../src/i18n.js'), 'utf8');
    const context = vm.createContext({ document, console });
    vm.runInContext(source, context);
    context.setLanguage('nl');
    assert.equal(label.textContent, 'CPU Gebruik');
    context.setLanguage('en');
    assert.equal(label.textContent, 'CPU Usage');
});
