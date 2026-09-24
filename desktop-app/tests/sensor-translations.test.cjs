const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

test('every backend sensor choice has English and Dutch labels', () => {
    const catalog = fs.readFileSync(path.join(__dirname, '../src-tauri/src/sensors/collector.rs'), 'utf8');
    const list = catalog.split('pub fn get_sensor_list(&self)')[1].split('pub fn set_enabled_sensors')[0];
    const ids = [...list.matchAll(/\("([a-z_]+)",\s*"[^"]+",\s*(?:true|false)\)/g)].map(match => match[1]);
    assert.ok(ids.length >= 20, 'sensor catalog was not found');

    const source = fs.readFileSync(path.join(__dirname, '../src/i18n.js'), 'utf8');
    const translations = vm.runInNewContext(`${source}\ntranslations`, { console });
    for (const id of ids) {
        for (const language of ['en', 'nl']) {
            assert.ok(translations[language][id], `${language} is missing ${id}`);
            assert.notEqual(translations[language][id], id, `${language} exposes raw key ${id}`);
        }
    }
});
