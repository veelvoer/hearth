'use strict';
// Renders every screen with sample data and saves screenshots to /tmp/cm_<tab>.png. Run: npx electron test/selftest.js [dark|light]
const { app, BrowserWindow, nativeTheme } = require('electron');
const fs = require('fs');
const path = require('path');
const theme = process.argv.includes('light') ? 'light' : 'dark';
app.whenReady().then(async () => {
  nativeTheme.themeSource = theme;
  const w = new BrowserWindow({ width: 1240, height: 1700, show: false, webPreferences: { offscreen: true, preload: path.join(__dirname, 'stub-preload.js'), contextIsolation: true } });
  const errors = [];
  w.webContents.on('console-message', (e) => { if (e.level === 'error' || e.level === 3) errors.push(e.message); });
  w.webContents.on('preload-error', (e, p, err) => errors.push('preload: ' + err.message));
  await w.loadFile(path.join(__dirname, '..', 'renderer', 'index.html'));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  await sleep(800);
  for (const tab of ['sessions', 'options', 'collapsed', 'newchat']) {
    try { await w.webContents.executeJavaScript(tab === 'newchat' ? `newChat()` : tab === 'options' ? `(SS.opts = true, drawPane(), loadStatus(SS.sel.mid))` : tab === 'collapsed' ? `(localStorage.setItem('cm.navCollapsed','1'), localStorage.setItem('cm.sideW','400'), setTab('dashboard'), setTab('sessions'))` : `setTab('${tab}')`); } catch (e) { errors.push(tab + ': ' + e.message); }
    await sleep(tab === 'sessions' ? 1500 : 900);
    if (tab === 'sessions') { await w.webContents.executeJavaScript(`openSession({id:'a1',title:'Fix the login handler tests',cwd:'/home/you/code/api-server',mid:'local',live:true})`).catch((e) => errors.push(e.message)); await sleep(900); }
    const img = await w.webContents.capturePage();
    fs.writeFileSync(`/tmp/cm_${theme}${process.env.CM_PALETTE ? '_' + process.env.CM_PALETTE : ''}_${tab}.png`, img.toPNG());
    if (tab === 'dashboard') for (const y of [620, 1240]) { await w.webContents.executeJavaScript(`document.querySelector('main').scrollTop=${y}`); await sleep(400); fs.writeFileSync(`/tmp/cm_${theme}_${tab}_${y}.png`, (await w.webContents.capturePage()).toPNG()); }
  }
  fs.writeFileSync('/tmp/cm_errors.txt', errors.join('\n') || 'no errors');
  app.quit();
});
