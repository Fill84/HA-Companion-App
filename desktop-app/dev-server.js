const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

const root = path.join(__dirname, 'dist');
const assets = new Map([
    ['/index.html', 'text/html; charset=utf-8'],
    ['/src/i18n.js', 'text/javascript; charset=utf-8'],
    ['/src/main.js', 'text/javascript; charset=utf-8'],
    ['/src/settings.js', 'text/javascript; charset=utf-8'],
    ['/src/particles.js', 'text/javascript; charset=utf-8'],
    ['/src/styles.css', 'text/css; charset=utf-8'],
    ['/icons/icon.png', 'image/png'],
    ['/icons/icon@2x.png', 'image/png'],
    ['/vendor/tsparticles.preset.links.bundle.min.js', 'text/javascript; charset=utf-8'],
    ['/vendor/tsparticles.preset.links.bundle.min.js.LICENSE.txt', 'text/plain; charset=utf-8'],
    ['/vendor/LICENSE.tsparticles.txt', 'text/plain; charset=utf-8'],
]);

function createDevServer() {
    return http.createServer((request, response) => {
        if (request.method !== 'GET' && request.method !== 'HEAD') {
            response.writeHead(405, { Allow: 'GET, HEAD' }).end();
            return;
        }

        let pathname;
        try {
            pathname = new URL(request.url, 'http://127.0.0.1').pathname;
        } catch {
            response.writeHead(400).end();
            return;
        }
        if (pathname === '/') pathname = '/index.html';
        const contentType = assets.get(pathname);
        if (!contentType) {
            response.writeHead(404).end();
            return;
        }

        const filePath = path.join(root, pathname.slice(1));
        fs.stat(filePath, (error, stats) => {
            if (error || !stats.isFile()) {
                response.writeHead(404).end();
                return;
            }
            response.writeHead(200, {
                'Content-Type': contentType,
                'Content-Length': stats.size,
                'Cache-Control': 'no-store',
                'X-Content-Type-Options': 'nosniff',
            });
            if (request.method === 'HEAD') {
                response.end();
            } else {
                fs.createReadStream(filePath).pipe(response);
            }
        });
    });
}

if (require.main === module) {
    const server = createDevServer();
    server.on('error', error => {
        console.error(`Development server could not start: ${error.message}`);
        process.exitCode = 1;
    });
    server.listen(1420, '127.0.0.1', () => {
        console.log('Development assets at http://127.0.0.1:1420');
    });
}

module.exports = { createDevServer };
