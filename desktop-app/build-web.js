const fs = require('fs');
const path = require('path');

// Remove dist folder if it exists
if (fs.existsSync('dist')) {
    fs.rmSync('dist', { recursive: true });
}

// Create dist folder structure
fs.mkdirSync('dist');
fs.mkdirSync('dist/src');
fs.mkdirSync('dist/icons');
fs.mkdirSync('dist/vendor');

// Copy index.html
fs.copyFileSync('index.html', 'dist/index.html');

// Copy all files from src folder
const files = ['i18n.js', 'main.js', 'settings.js', 'particles.js', 'styles.css'];
files.forEach(file => {
    fs.copyFileSync(path.join('src', file), path.join('dist/src', file));
});

// Copy icons used by index.html (integration brand logo + retina variant)
const iconFiles = ['icon.png', 'icon@2x.png'];
iconFiles.forEach(file => {
    fs.copyFileSync(path.join('icons', file), path.join('dist/icons', file));
});

// Copy vendored third-party JS (tsparticles bundle for HA-style background)
const vendorFiles = ['tsparticles.preset.links.bundle.min.js'];
vendorFiles.forEach(file => {
    fs.copyFileSync(path.join('vendor', file), path.join('dist/vendor', file));
});

console.log('Web assets copied to dist folder');
