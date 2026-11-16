// The Tauri CLI wants the page folder to exist before it builds; build.rs fills it.
require('fs').mkdirSync(require('path').join(__dirname, 'web'), { recursive: true });
