const fs = require('fs');
const path = require('path');

// Remove dist folder if it exists
if (fs.existsSync('dist')) {
    fs.rmSync('dist', { recursive: true });
}

// Create dist folder structure
fs.mkdirSync('dist');
fs.mkdirSync('dist/src');

// Copy index.html
fs.copyFileSync('index.html', 'dist/index.html');

// Copy all files from src folder
const files = ['i18n.js', 'main.js', 'settings.js', 'styles.css'];
files.forEach(file => {
    fs.copyFileSync(path.join('src', file), path.join('dist/src', file));
});

console.log('Web assets copied to dist folder');
