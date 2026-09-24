const test = require('node:test');
const assert = require('node:assert/strict');
const { once } = require('node:events');
const { createDevServer } = require('../dev-server.js');

test('development server binds loopback and serves only bundled assets', async () => {
    const server = createDevServer();
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    try {
        const address = server.address();
        assert.equal(address.address, '127.0.0.1');
        const base = `http://127.0.0.1:${address.port}`;
        const index = await fetch(base);
        assert.equal(index.status, 200);
        assert.match(await index.text(), /Home Assistant Companion/);
        assert.equal((await fetch(`${base}/src/main.js`)).status, 200);
        assert.equal((await fetch(`${base}/package.json`)).status, 404);
        assert.equal((await fetch(`${base}/src-tauri/Cargo.toml`)).status, 404);
        assert.equal((await fetch(`${base}/../src-tauri/Cargo.toml`)).status, 404);
        assert.equal((await fetch(base, { method: 'POST' })).status, 405);
    } finally {
        server.close();
        await once(server, 'close');
    }
});

test('occupied development port fails instead of selecting a network port', async () => {
    const first = createDevServer();
    first.listen(0, '127.0.0.1');
    await once(first, 'listening');
    const second = createDevServer();
    try {
        second.listen(first.address().port, '127.0.0.1');
        const [error] = await once(second, 'error');
        assert.equal(error.code, 'EADDRINUSE');
        assert.equal(second.listening, false);
    } finally {
        first.close();
        await once(first, 'close');
    }
});
