'use strict';
// Layout audit: opens every screen at several window sizes and reports elements that stick out of the window or clip their text.
// Run: node_modules/.bin/electron test/layout.js   Output: /tmp/hl_layout.txt and screenshots /tmp/hl_<size>_<screen>.png
const { app, BrowserWindow, nativeTheme } = require('electron');
const fs = require('fs');
const path = require('path');
const SIZES = [[820, 560], [1120, 780], [1500, 900]];
const SCREENS = [
  ['dashboard', `setTab('dashboard')`], ['now', `setTab('now')`], ['history', `setTab('history')`], ['settings', `setTab('settings')`],
  ['chat', `openSession({id:'a1',title:'Fix the login handler tests and update the documentation for the whole API',cwd:'/home/you/code/api-server',mid:SS.mid,mtime:Date.now(),live:false})`],
  ['newchat', `setTab('sessions'); newChat()`],
  ['tools-skills', `openTools(SS.sel||{mid:SS.mid,cwd:'/x'},'skills')`], ['tools-plugins', `openTools(PT.s,'plugins')`], ['tools-conn', `openTools(PT.s,'connections')`], ['tools-changes', `openTools(PT.s,'changes')`], ['tools-notes', `openTools(PT.s,'notes')`],
  ['support-home', `openSupport('home')`], ['support-new', `openSupport('new')`], ['support-thread', `SP.open='HRT-48213'; openSupport('thread')`], ['support-login', `SP.signedIn=false; SP.view='login'; drawSupport()`], ['guide1', `if (OB.el) { OB.el.remove(); OB.el = null; } startTour()`], ['guide3', `(async()=>{ await showStep(3); })()`], ['guide5', `(async()=>{ await showStep(5); })()`], ['guide9', `(async()=>{ await showStep(9); })()`], ['tour0', `closeTools(); OB.step=0; startTutorial()`], ['tour1', `go(1)`], ['tour2', `go(2)`], ['tour3', `go(3)`], ['tour4', `go(4)`], ['tour5', `go(5)`], ['tour6', `go(6)`], ['tour7', `go(7)`],
];
const AUDIT = `(() => {
  const W = innerWidth, H = innerHeight, out = [];
  const name = (e) => e.tagName.toLowerCase() + (e.id ? '#' + e.id : '') + (e.className && typeof e.className === 'string' ? '.' + e.className.trim().split(/\\s+/).join('.') : '') + ' "' + (e.textContent || '').trim().slice(0, 30) + '"';
  for (const e of document.querySelectorAll('body *')) {
    const cs = getComputedStyle(e); if (cs.display === 'none' || cs.visibility === 'hidden' || e.closest('[hidden]')) continue;
    const r = e.getBoundingClientRect(); if (!r.width || !r.height) continue;
    let p = e.parentElement, scrolls = false; while (p) { const o = getComputedStyle(p); if (/(auto|scroll)/.test(o.overflowY + o.overflowX) && p !== document.body) scrolls = true; p = p.parentElement; }
    if (r.right > W + 1 && !scrolls) out.push('off-right ' + Math.round(r.right - W) + 'px: ' + name(e));
    if (r.left < -1 && !scrolls) out.push('off-left: ' + name(e));
    if (e.scrollWidth > e.clientWidth + 2 && cs.overflowX === 'visible' && e.children.length === 0 && cs.display !== 'inline') out.push('text-clipped ' + (e.scrollWidth - e.clientWidth) + 'px: ' + name(e));
    if (/(INPUT|BUTTON|SELECT|TEXTAREA)/.test(e.tagName) && !scrolls && (r.bottom > H + 1)) out.push('below-window: ' + name(e));
  }
  return [...new Set(out)].slice(0, 25);
})()`;
app.whenReady().then(async () => {
  nativeTheme.themeSource = process.argv.includes('light') ? 'light' : 'dark';
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const report = [];
  const win = new BrowserWindow({ width: 1120, height: 780, show: false, webPreferences: { offscreen: true, preload: path.join(__dirname, 'stub-preload.js'), contextIsolation: true } });
  const errors = []; win.webContents.on('console-message', (e) => { if (e.level === 'error' || e.level === 3) errors.push(e.message); });
  await win.loadFile(path.join(__dirname, '..', 'renderer', 'index.html')); await sleep(900);
  await win.webContents.executeJavaScript(`S.settings.onboarded = true; 1`);
  for (const [w, h] of SIZES) {
    win.setContentSize(w, h); await sleep(400);
    for (const [name, code] of SCREENS) {
      try { if (!/^tour[1-9]/.test(name) && !/^guide[2-9]/.test(name)) await win.webContents.executeJavaScript('if (OB.el) { OB.el.remove(); OB.el = null; } if (typeof TOUR !== \"undefined\" && TOUR.el) endTour(); closeTools(); if (typeof closeSupport === "function") closeSupport(); 1'); await win.webContents.executeJavaScript(code); } catch (e) { report.push(`[${w}x${h}] ${name}: SCRIPT ERROR ${e.message}`); continue; }
      await sleep(700);
      const issues = await win.webContents.executeJavaScript(AUDIT);
      fs.writeFileSync(`/tmp/hl_${w}_${name}.png`, (await win.webContents.capturePage()).toPNG());
      report.push(`[${w}x${h}] ${name}: ${issues.length ? '\n   ' + issues.join('\n   ') : 'ok'}`);
    }
  }
  if (errors.length) report.push('console errors: ' + [...new Set(errors)].join(' | '));
  win.destroy();
  fs.writeFileSync('/tmp/hl_layout.txt', report.join('\n'));
  app.quit();
});
