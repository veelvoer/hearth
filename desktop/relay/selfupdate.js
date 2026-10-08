'use strict';
/* Looks for a newer Hearth on GitHub and, on a server, replaces its own program files with the newer ones.
   Only ever talks to github.com/veelvoer/hearth. The new files are checked before they replace the old ones. */
const https = require('https');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawn } = require('child_process');

const REPO = 'veelvoer/hearth';
const UA = 'Hearth-updater';

function get(url, { json = false, redirects = 5, ms = 20000 } = {}) {
  return new Promise((resolve, reject) => {
    const u = new URL(url);
    if (u.protocol !== 'https:' || !/(^|\.)(github\.com|githubusercontent\.com|api\.github\.com)$/.test(u.hostname)) return reject(new Error('refusing to fetch ' + u.hostname));
    const r = https.get(url, { headers: { 'User-Agent': UA, Accept: json ? 'application/vnd.github+json' : '*/*' }, timeout: ms }, (rs) => {
      if (rs.statusCode >= 300 && rs.statusCode < 400 && rs.headers.location && redirects > 0) { rs.resume(); return resolve(get(new URL(rs.headers.location, url).toString(), { json, redirects: redirects - 1, ms })); }
      if (rs.statusCode !== 200) { rs.resume(); return reject(new Error('GitHub said ' + rs.statusCode)); }
      if (!json) return resolve(rs);
      let b = ''; rs.on('data', (d) => { b += d; }); rs.on('end', () => { try { resolve(JSON.parse(b)); } catch (e) { reject(e); } });
    });
    r.on('error', reject); r.on('timeout', () => r.destroy(new Error('GitHub did not answer')));
  });
}
const parse = (v) => String(v || '').replace(/^v/, '').split('.').map((n) => parseInt(n, 10) || 0);
const newer = (a, b) => { const x = parse(a), y = parse(b); for (let i = 0; i < 3; i++) { if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) > (y[i] || 0); } return false; };

/** The newest release: { tag, version, notes, assets: [{ name, url, size }] } */
async function latest() {
  const r = await get(`https://api.github.com/repos/${REPO}/releases/latest`, { json: true });
  return { tag: r.tag_name, version: String(r.tag_name || '').replace(/^v/, ''), notes: String(r.body || '').slice(0, 1500), assets: (r.assets || []).map((a) => ({ name: a.name, url: a.browser_download_url, size: a.size })) };
}

const run = (cmd, args, o = {}) => new Promise((resolve) => { const p = spawn(cmd, args, o); let out = ''; p.stdout && p.stdout.on('data', (d) => { out += d; }); p.stderr && p.stderr.on('data', (d) => { out += d; }); p.on('error', (e) => resolve({ ok: false, out: e.message })); p.on('close', (c) => resolve({ ok: c === 0, out })); });

/** Replaces the program files in relayDir with the ones of release `tag`. Returns the new version. The caller restarts the program. */
async function apply(tag, relayDir) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error('bad version');
  try { fs.accessSync(relayDir, fs.constants.W_OK); } catch { throw new Error('This installation cannot update itself. On the server run: hearth-server update'); }
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'hearth-update-'));
  try {
    const rs = await get(`https://github.com/${REPO}/archive/${tag}.tar.gz`, { ms: 60000 });
    const tar = spawn('tar', ['-xz', '-C', tmp, '--strip-components=1', '--wildcards', '*/desktop/relay/*.js', '*/server/hearth-server.js']);
    let err = ''; tar.stderr.on('data', (d) => { err += d; });
    rs.pipe(tar.stdin);
    const code = await new Promise((res) => tar.on('close', res));
    if (code !== 0) throw new Error('could not unpack the update: ' + err.trim().slice(0, 120));
    const src = path.join(tmp, 'desktop', 'relay');
    const files = fs.readdirSync(src).filter((f) => f.endsWith('.js') && f !== 'embedded.js');
    for (const need of ['server.js', 'standalone.js', 'version.js', 'selfupdate.js']) if (!files.includes(need)) throw new Error('the update is incomplete (' + need + ' is missing)');
    const want = tag.replace(/^v/, '');
    if (require(path.join(src, 'version.js')) !== want) throw new Error('the update says it is another version than ' + want);
    for (const f of files) { const c = await run(process.execPath, ['--check', path.join(src, f)]); if (!c.ok) throw new Error('the update did not pass a check (' + f + ')'); }
    const prev = relayDir + '.prev'; fs.rmSync(prev, { recursive: true, force: true }); fs.cpSync(relayDir, prev, { recursive: true });
    for (const f of files) { const t = path.join(relayDir, f + '.new'); fs.copyFileSync(path.join(src, f), t); fs.renameSync(t, path.join(relayDir, f)); }
    const inst = path.join(tmp, 'server', 'hearth-server.js'), instDest = path.join(relayDir, '..', 'installer', 'hearth-server.js');
    try { if (fs.existsSync(inst) && fs.existsSync(path.dirname(instDest))) fs.copyFileSync(inst, instDest); } catch { /* the installer is optional */ }
    return want;
  } finally { fs.rmSync(tmp, { recursive: true, force: true }); }
}
module.exports = { latest, apply, newer, get };
