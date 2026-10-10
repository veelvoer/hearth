'use strict';
/* "A new version is ready": notices when the project files on this computer (synced from your server) are newer than
   what is running or installed, and updates everything in one go: the server, the phone app (build + install over USB),
   and this app with its background relay. Only meant for the source checkout; a packaged app has nothing to rebuild. */
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { spawn, execFile } = require('child_process');
const { sigOfDir } = require('./relay/buildsig');
const gh = require('./relay/selfupdate');

const SKIP = new Set(['node_modules', 'dist', 'build', '.gradle', '.git', 'test', 'diagnostics', '.uploads']);

function walk(dir, out, depth = 0) {
  let ents = [];
  try { ents = fs.readdirSync(dir, { withFileTypes: true }); } catch { return out; }
  for (const e of ents) {
    if (SKIP.has(e.name) || e.name.startsWith('.')) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) { if (depth < 12) walk(p, out, depth + 1); } else out.push(p);
  }
  return out;
}
function hashFiles(files) {
  const h = crypto.createHash('sha1');
  for (const f of files.sort()) {
    try { h.update(f + '\0'); h.update(fs.readFileSync(f)); } catch { /* vanished */ }
  }
  return h.digest('hex').slice(0, 16);
}

/** @param {{root:string, desktopDir:string, getState:()=>any, save:()=>void, send:(ch:string,data:any)=>void, serverBusy:()=>Promise<boolean>, localBusy:()=>Promise<boolean>, relaunch:()=>void}} d */
function create(d) {
  const { root, desktopDir } = d;
  const isWin = process.platform === 'win32';
  const hasAndroid = fs.existsSync(path.join(root, isWin ? 'gradlew.bat' : 'gradlew')) && fs.existsSync(path.join(root, 'app', 'src'));
  const hasServer = !isWin && fs.existsSync(path.join(root, 'server', 'deploy.sh'));
  const hasSource = fs.existsSync(path.join(desktopDir, 'main.js')) && !/\.asar([\\/]|$)/.test(desktopDir);

  const desktopFiles = () => [...['main.js', 'preload.js', 'updater.js', 'package.json'].map((f) => path.join(desktopDir, f)), ...walk(path.join(desktopDir, 'relay'), []), ...walk(path.join(desktopDir, 'renderer'), [])];
  const phoneFiles = () => [...walk(path.join(root, 'app', 'src'), []), ...['app/build.gradle.kts', 'build.gradle.kts', 'settings.gradle.kts'].map((f) => path.join(root, f))];
  const sigs = () => ({ desktop: hashFiles(desktopFiles()), phone: hasAndroid ? hashFiles(phoneFiles()) : '' });
  const relaySig = () => sigOfDir(path.join(desktopDir, 'relay'));   // what the server and this computer's relay should be running

  const releaseMode = !hasSource;   // an installed app updates from GitHub releases; a source checkout updates from its own files
  let ghCache = null;
  const latest = async () => { if (!ghCache || Date.now() - ghCache.at > 60000) ghCache = { at: Date.now(), r: await gh.latest().catch(() => null) }; return ghCache.r; };
  async function computeRelease() {
    const l = await latest(), cur = d.version(), sv = await d.relayStatus('server');   // sv: object, null (unreachable) or undefined (no server)
    const server = !!(l && sv && sv.version && sv.canSelfUpdate && gh.newer(l.version, sv.version));
    return { mode: 'release', latest: l && l.version, desktop: !!(l && gh.newer(l.version, cur)), phone: false, server, cur, sv, l };
  }

  // a source checkout that came from GitHub can pull the newest code, then rebuild everything from it
  const isRepo = hasSource && fs.existsSync(path.join(root, '.git'));
  const git = (...args) => new Promise((resolve) => execFile('git', ['-C', root, ...args], { timeout: 25000 }, (e, out, err) => resolve({ ok: !e, out: String(out || '').trim(), err: String(err || (e && e.message) || '').trim() })));
  async function behindGithub() {
    if (!isRepo) return null;
    const f = await git('fetch', '--quiet', 'origin'); if (!f.ok) return null;
    const n = await git('rev-list', '--count', 'HEAD..origin/main'); return n.ok ? Number(n.out) || 0 : null;
  }

  const loaded = hasSource ? sigs().desktop : '';   // what this running app was started from
  let running = false, dismissedAt = 0, last = '';

  /** What is out of date. The server and this computer's relay are asked what they really run; the phone is judged by the last install from here. */
  async function compute() {
    if (releaseMode) return computeRelease();
    const sg = sigs(), dep = d.getState().deployed || {}, rs = relaySig();
    const srv = hasServer ? await d.relayBuild('server') : undefined;     // string = build, '' = older relay, null = unreachable, undefined = none
    const loc = await d.relayBuild('local');
    const localNeed = typeof loc === 'string' && loc !== rs;
    const behind = await behindGithub();
    return { desktop: sg.desktop !== loaded || localNeed, phone: false, server: typeof srv === 'string' && srv !== rs, github: behind > 0, behind, sigs: sg, rs, srv, loc };
  }
  const summary = (n) => (n && (n.desktop || n.phone || n.server || n.github) ? { desktop: n.desktop, phone: n.phone, server: n.server, github: !!n.github, behind: n.behind, canPhone: hasAndroid, canServer: hasServer || releaseMode, mode: n.mode || 'source', latest: n.latest } : null);

  async function check(force = false) {
    if (running) return;
    const n = summary(await compute());
    if (!n) { last = ''; return; }
    if (dismissedAt && Date.now() - dismissedAt >= 30 * 60000) { dismissedAt = 0; last = ''; }
    const key = JSON.stringify(n);
    if (!force && (key === last || (dismissedAt && Date.now() - dismissedAt < 30 * 60000))) return;
    last = key; d.send('update-available', n);
    if (d.windowHidden && d.windowHidden() && d.notify) d.notify('An update is ready', 'Open Hearth to update the server, your phone and this computer.');
  }

  /** For the Settings screen: the state of each part, in words. */
  async function details() {
    if (releaseMode) {
      const n = await computeRelease();
      if (!n.l) return { unavailable: 'Could not look for updates right now. Check your internet connection.', checkedAt: Date.now() };
      const srv = n.sv === undefined ? { na: true, text: 'No server connected' } : n.sv === null ? { na: true, text: 'The server can\'t be reached right now' }
        : !n.sv.version ? { need: false, na: true, text: 'Older server: run the installer line again to update it' }
        : n.server ? { need: true, text: `Version ${n.sv.version} · update to ${n.latest}` } : gh.newer(n.latest, n.sv.version) ? { na: true, text: 'Version ' + n.sv.version + ' · update it on the server with: hearth-server update' } : { text: 'Version ' + n.sv.version + ' · up to date' };
      return { mode: 'release', checkedAt: Date.now(), desktop: { need: n.desktop, text: n.desktop ? `Version ${n.cur} · update to ${n.latest}` : `Version ${n.cur} · up to date` }, phone: { na: true, text: 'Update it inside the phone app (Settings → Updates)' }, server: srv };
    }
    const n = await compute(), dep = d.getState().deployed || {};
    const gh1 = n.behind === null || n.behind === undefined ? undefined : { need: n.behind > 0, text: n.behind > 0 ? n.behind + (n.behind === 1 ? ' newer change' : ' newer changes') + ' on GitHub' : 'Up to date with GitHub' };
    const when = dep.phoneAt ? new Date(dep.phoneAt).toLocaleDateString() : '';
    return {
      checkedAt: Date.now(), ...(gh1 ? { github: gh1 } : {}),
      desktop: { need: n.desktop, text: n.desktop ? 'Update ready' : 'Up to date' },
      phone: { na: true, text: 'Updates itself from GitHub: open Settings → Updates in the phone app' },
      server: !hasServer ? { na: true, text: 'No server set up on this computer' } : n.srv === undefined ? { na: true, text: 'No server connected' } : n.srv === null ? { na: true, text: 'The server can\'t be reached right now' }
        : { need: n.server, text: n.server ? 'Update ready' + (n.srv === '' ? ' · running an older version' : '') : 'Up to date' },
    };
  }

  const sh = (cmd, args, opts, onLine) => new Promise((resolve, reject) => {
    const p = spawn(cmd, args, { ...opts, stdio: ['ignore', 'pipe', 'pipe'] });
    let tail = '';
    const eat = (b) => { tail = (tail + b.toString()).slice(-1500); if (onLine) onLine(b.toString()); };
    p.stdout.on('data', eat); p.stderr.on('data', eat);
    p.on('error', reject);
    p.on('close', (code) => (code === 0 ? resolve(tail) : reject(new Error(tail.trim().split('\n').slice(-4).join('\n') || `${cmd} exited with ${code}`))));
  });

  function adbPath() {
    const homes = [process.env.ANDROID_HOME, process.env.ANDROID_SDK_ROOT, path.join(os.homedir(), 'Android', 'sdk'), path.join(os.homedir(), 'Android', 'Sdk'), path.join(process.env.LOCALAPPDATA || '', 'Android', 'Sdk')].filter(Boolean);
    for (const h of homes) { const p = path.join(h, 'platform-tools', isWin ? 'adb.exe' : 'adb'); if (fs.existsSync(p)) return p; }
    return 'adb';
  }
  const adbDevices = (adb) => new Promise((resolve) => execFile(adb, ['devices'], { timeout: 8000 }, (e, out) => resolve(e ? null : String(out).split('\n').slice(1).filter((l) => /\tdevice$/.test(l.trim() + '')).map((l) => l.split('\t')[0]))));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  const download = (url, file, onPct) => new Promise((resolve, reject) => {
    gh.get(url, { ms: 120000 }).then((rs) => {
      const total = Number(rs.headers['content-length']) || 0; let got = 0, last = 0;
      const out = fs.createWriteStream(file);
      rs.on('data', (c) => { got += c.length; if (total && Date.now() - last > 300) { last = Date.now(); onPct(Math.floor(got * 100 / total)); } });
      rs.pipe(out); out.on('finish', () => resolve()); out.on('error', reject); rs.on('error', reject);
    }, reject);
  });
  async function installDesktop(l, say) {
    const pick = (re) => l.assets.find((a) => re.test(a.name));
    const tmp = path.join(os.tmpdir(), 'hearth-update'); fs.mkdirSync(tmp, { recursive: true });
    if (process.platform === 'linux' && process.env.APPIMAGE) {
      const a = pick(/AppImage$/); if (!a) throw new Error('No AppImage in the newest release.');
      const target = process.env.APPIMAGE, part = target + '.new';
      await download(a.url, part, (p) => say('desktop', `Downloading the new version… ${p}%`)); fs.chmodSync(part, 0o755); fs.renameSync(part, target);
      return { relaunch: target };
    }
    if (process.platform === 'win32' && !process.env.PORTABLE_EXECUTABLE_FILE) {
      const a = pick(/win-x64\.exe$/); if (!a) throw new Error('No installer in the newest release.');
      const file = path.join(tmp, a.name); await download(a.url, file, (p) => say('desktop', `Downloading the new version… ${p}%`));
      spawn('cmd', ['/c', 'start', '/wait', '""', file, '/S', '&&', 'start', '""', process.execPath], { detached: true, stdio: 'ignore', windowsVerbatimArguments: true }).unref();
      return { quit: true };
    }
    if (process.platform === 'linux') {   // installed from a .deb: the system installer asks for the password itself
      const a = pick(/\.deb$/); if (!a) throw new Error('No package in the newest release.');
      const file = path.join(os.homedir(), 'Downloads', a.name); fs.mkdirSync(path.dirname(file), { recursive: true });
      await download(a.url, file, (p) => say('desktop', `Downloading the new version… ${p}%`));
      d.openPath(file); return { manual: 'The installer is open. Press Install, then start Hearth again.' };
    }
    d.openExternal('https://github.com/veelvoer/hearth/releases/latest'); return { manual: 'The download page is open in your browser.' };
  }
  async function runRelease(want, say) {
    const l = await latest(); let failed = false, relaunch = null;
    if (want.server) {
      say('server', 'Updating the server…');
      try {
        if (await d.serverBusy()) throw new Error('Claude is working on the server right now. Try again when it has finished.');
        const r = await d.relayPost('server', '/admin/update', {});
        if (r && r.restarting) { for (let i = 0; i < 30; i++) { await sleep(1500); const sv = await d.relayStatus('server').catch(() => null); if (sv && sv.version === r.to) break; } }
        say('server', 'Server updated', 'ok');
      } catch (e) { failed = true; say('server', e.message, 'fail'); }
    }
    if (want.desktop && l) {
      try {
        if (await d.localBusy()) throw new Error('Claude is working on this computer right now. Try again when it has finished.');
        say('desktop', 'Downloading the new version…');
        const r = await installDesktop(l, say);
        if (r.manual) say('desktop', r.manual, 'ok'); else { say('desktop', 'Restarting…', 'ok'); relaunch = r; }
      } catch (e) { failed = true; say('desktop', e.message, 'fail'); }
    }
    return { failed, relaunch };
  }

  async function run(want) {
    if (running) return;
    running = true;
    if (releaseMode) {
      const say = (step, text, state = 'run') => d.send('update-progress', { step, text, state });
      let res = { failed: true };
      try { res = await runRelease(want, say); } finally {
        running = false;
        if (res.relaunch) { await sleep(1200); d.relaunch(res.relaunch.relaunch || null); }
        else { d.send('update-progress', { step: 'end', state: res.failed ? 'fail' : 'ok' }); if (!res.failed) last = ''; }
      }
      return;
    }
    const say = (step, text, state = 'run') => d.send('update-progress', { step, text, state });
    let failed = false;
    try {
      if (want.github && isRepo) {
        say('github', 'Getting the newest code from GitHub…');
        const p = await git('pull', '--ff-only', 'origin', 'main');
        if (p.ok) say('github', 'Newest code downloaded', 'ok'); else { failed = true; say('github', /local changes|overwritten/i.test(p.err) ? 'You have unsaved changes in this folder, so GitHub\'s code was not pulled.' : p.err.split('\n').slice(-2).join(' '), 'fail'); }
      }
      const n = (await compute()) || { sigs: sigs() };
      if (want.github) { want = { ...want, server: want.server || !!n.server, phone: want.phone || !!n.phone, desktop: want.desktop || !!n.desktop }; }   // after pulling, whatever is now out of date gets updated too
      const sg = n.sigs;
      if (want.server && hasServer) {
        say('server', 'Updating the server…');
        try {
          if (await d.serverBusy()) throw new Error('Claude is working on the server right now. Try again when it has finished.');
          await sh('bash', [path.join(root, 'server', 'deploy.sh')], { cwd: root, env: { ...process.env, SSH_ASKPASS_REQUIRE: 'never' } });
          say('server', 'Server updated', 'ok');
        } catch (e) { failed = true; say('server', e.message, 'fail'); }
      }
      if (want.phone && hasAndroid) {
        try {
          say('phone', 'Building the phone app… (a few minutes)');
          await sh(path.join(root, isWin ? 'gradlew.bat' : 'gradlew'), [':app:assembleDebug', '-q'], { cwd: root, shell: isWin });
          const apk = path.join(root, 'app', 'build', 'outputs', 'apk', 'debug', 'app-debug.apk');
          if (!fs.existsSync(apk)) throw new Error('The build finished but no app file was made.');
          const adb = adbPath();
          let devs = await adbDevices(adb);
          if (devs === null) throw new Error('adb was not found. Install the Android platform-tools.');
          for (let i = 0; i < 60 && devs.length === 0; i++) {
            say('phone', 'Plug in your phone with USB debugging on…');
            await sleep(3000); devs = await adbDevices(adb);
          }
          if (!devs.length) throw new Error('No phone found. Plug it in (USB debugging on) and press Update again.');
          say('phone', 'Installing on your phone…');
          await sh(adb, ['-s', devs[0], 'install', '-r', apk], {});
          const s = d.getState(); s.deployed = { ...(s.deployed || {}), phone: sg.phone, phoneAt: Date.now() }; d.save();
          say('phone', 'Phone updated', 'ok');
        } catch (e) { failed = true; say('phone', e.message, 'fail'); }
      }
      if (want.desktop && hasSource) {   // also restarts the background relay on this computer
        try {
          if (await d.localBusy()) throw new Error('Claude is working on this computer right now. Try again when it has finished.');
          say('desktop', 'Updating this app…');
          if (!isWin && fs.existsSync(path.join(root, 'relay', 'update-local.sh'))) await sh('bash', [path.join(root, 'relay', 'update-local.sh')], { cwd: root });
          say('desktop', failed ? 'Restarting this app (some steps failed, see above)…' : 'Restarting…', 'ok');
          await sleep(failed ? 6000 : 1200);
          running = false; d.relaunch(); return;
        } catch (e) { failed = true; say('desktop', e.message, 'fail'); }
      }
    } finally { if (running) { running = false; d.send('update-progress', { step: 'end', state: failed ? 'fail' : 'ok' }); if (!failed) { last = ''; } } }
  }

  return { check, run, later: () => { dismissedAt = Date.now(); }, status: async () => summary(await compute()), details, _sigs: sigs };
}

module.exports = { create };
