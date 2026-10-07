'use strict';
// Makes the README screenshots with sample data. Run: HEARTH_SHOTS=1 node_modules/.bin/electron test/shots.js  (writes to ../docs/screenshots)
const { app, BrowserWindow, nativeTheme } = require('electron');
const fs = require('fs');
const path = require('path');
const OUT = path.join(__dirname, '..', '..', 'docs', 'screenshots');
const SHOTS = [
  ['desktop-usage', `setTab('dashboard')`],
  ['desktop-chat', `openSession({id:'a1',title:'Fix the login handler tests',cwd:'/home/you/projects/api-server',mid:SS.mid,mtime:Date.now(),live:false})`],
  ['desktop-addons', `openTools(SS.sel,'plugins')`],
  ['desktop-changes', `openTools(SS.sel,'changes')`],
  ['desktop-setup', `closeTools(); OB.step=1; startTutorial()`],
  ['desktop-sync', `if (OB.el) { OB.el.remove(); OB.el = null; } setTab('settings')`],
];
app.whenReady().then(async () => {
  nativeTheme.themeSource = 'dark'; fs.mkdirSync(OUT, { recursive: true });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const win = new BrowserWindow({ width: 1280, height: 800, show: false, webPreferences: { offscreen: true, preload: path.join(__dirname, 'stub-preload.js'), contextIsolation: true } });
  await win.loadFile(path.join(__dirname, '..', 'renderer', 'index.html')); await sleep(900);
  await win.webContents.executeJavaScript('S.settings.onboarded = true; 1');
  for (const [name, code] of SHOTS) {
    await win.webContents.executeJavaScript(code); await sleep(900);
    fs.writeFileSync(path.join(OUT, name + '.png'), (await win.webContents.capturePage()).toPNG());
  }
  app.quit();
});
