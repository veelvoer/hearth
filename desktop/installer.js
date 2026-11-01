'use strict';
/* Puts a newer Hearth in place of the running one, for every way the app can be installed on a computer.
   Rules: the old version keeps working until the new one is complete and checked; anything that fails is undone; when a way is not possible, the next one is tried
   and the person is told in plain words what happened and what to do. No electron here, so it can be tested on its own. */
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { execFile } = require('child_process');

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const writable = (p) => { try { fs.accessSync(p, fs.constants.W_OK); return true; } catch { return false; } };

/** How is this Linux copy of Hearth installed? appimage: one file run as an AppImage. extracted: an AppImage that was unpacked into a folder (squashfs-root). system: a package under /usr or /opt. */
function linuxKind({ env = process.env, execPath = process.execPath } = {}) {
  if (env.APPIMAGE) return { kind: 'appimage', file: env.APPIMAGE };
  let dir = path.dirname(execPath);
  if (env.APPDIR && fs.existsSync(path.join(env.APPDIR, 'AppRun'))) dir = env.APPDIR;   // started through AppRun of an unpacked AppImage
  const unpacked = fs.existsSync(path.join(dir, 'resources')) && (fs.existsSync(path.join(dir, 'AppRun')) || fs.existsSync(path.join(dir, 'resources', 'app.asar')));
  if (unpacked && !/^\/(usr|opt)(\/|$)/.test(dir)) return { kind: 'extracted', dir, viaAppRun: !!env.APPDIR && fs.existsSync(path.join(dir, 'AppRun')) };
  if (/^\/(usr|opt)(\/|$)/.test(execPath)) return { kind: 'system' };
  return { kind: 'unknown' };
}

/** Checks a downloaded file against what GitHub says about it (size, and the sha256 when GitHub gives one). */
async function verify(file, asset) {
  const st = fs.statSync(file);
  if (asset && asset.size && st.size !== asset.size) throw new Error(`The download is incomplete (${st.size} of ${asset.size} bytes).`);
  const m = asset && /^sha256:([0-9a-f]{64})$/i.exec(asset.digest || '');
  if (m) {
    const h = crypto.createHash('sha256');
    await new Promise((res, rej) => fs.createReadStream(file).on('data', (c) => h.update(c)).on('end', res).on('error', rej));
    if (h.digest('hex') !== m[1].toLowerCase()) throw new Error('The download is damaged (its checksum does not match).');
  }
}

/** Downloads [asset] to [file] (through a .part file), tries again on a bad connection, and checks the result. [get] is the https getter of selfupdate.js. */
async function download({ get, asset, file, onPct = () => {}, tries = 3, wait = 1500 }) {
  const part = file + '.part'; let lastErr;
  try {
    const free = fs.statfsSync(path.dirname(file)); if (asset.size && free.bavail * free.bsize < asset.size * 1.2) throw new Error('There is not enough free disk space for the update (' + Math.ceil(asset.size / 1e6) + ' MB needed).');
  } catch (e) { if (/free disk space/.test(e.message)) throw e; }
  for (let i = 0; i < tries; i++) {
    try {
      const rs = await get(asset.url, { ms: 120000 });
      const total = Number(rs.headers['content-length']) || asset.size || 0; let got = 0, last = 0;
      await new Promise((resolve, reject) => { const out = fs.createWriteStream(part); rs.on('data', (c) => { got += c.length; if (total && Date.now() - last > 300) { last = Date.now(); onPct(Math.floor(got * 100 / total)); } }); rs.on('error', reject); out.on('error', reject); out.on('finish', resolve); rs.pipe(out); });
      await verify(part, asset); fs.renameSync(part, file); return file;
    } catch (e) { lastErr = e; try { fs.unlinkSync(part); } catch { /* none */ } if (/free disk space/.test(e.message) || e.code === 'EACCES' || e.code === 'ENOSPC') break; await sleep(wait * (i + 1)); }
  }
  const m = lastErr && lastErr.message ? lastErr.message : '';
  throw new Error(!m ? 'The download failed.' : /aborted|ECONNRESET|socket hang up|ETIMEDOUT|ENOTFOUND|EAI_AGAIN/i.test(m) ? 'The connection was interrupted while downloading. Check your internet connection and try again.' : m);
}

/** An AppImage run as one file: the new file is made next to it and then takes its place in one step (a running AppImage can be replaced). */
function replaceFile(target, newFile) {
  fs.chmodSync(newFile, 0o755);
  try { fs.renameSync(newFile, target); } catch (e) { if (e.code !== 'EXDEV') throw e; fs.copyFileSync(newFile, target + '.copy'); fs.chmodSync(target + '.copy', 0o755); fs.renameSync(target + '.copy', target); fs.unlinkSync(newFile); }
  return target;
}

/** An unpacked AppImage (a folder): unpack the new AppImage next to it, check it, swap the folders, undo everything when a step fails. */
async function swapUnpacked({ dir, appImage, env = process.env, timeout = 180000 }) {
  const parent = path.dirname(dir);
  if (!writable(dir) || !writable(parent)) throw Object.assign(new Error('The folder of the app is read-only: ' + dir), { code: 'EACCES' });
  const work = fs.mkdtempSync(path.join(parent, '.hearth-update-'));
  const cleanup = () => { try { fs.rmSync(work, { recursive: true, force: true }); } catch { /* best effort */ } };
  try {
    fs.chmodSync(appImage, 0o755);
    await new Promise((resolve, reject) => execFile(appImage, ['--appimage-extract'], { cwd: work, timeout, env: { ...env, APPIMAGE_EXTRACT_AND_RUN: '1' }, maxBuffer: 8e6 }, (e, out, err) => (e ? reject(new Error('Could not unpack the new version: ' + String(err || e.message).slice(0, 200))) : resolve())));
    const fresh = path.join(work, 'squashfs-root');
    for (const need of ['AppRun', 'resources']) if (!fs.existsSync(path.join(fresh, need))) throw new Error('The new version is incomplete (' + need + ' is missing). Nothing was changed.');
    const exe = fs.readdirSync(fresh).find((n) => n !== 'AppRun' && (fs.statSync(path.join(fresh, n)).mode & 0o111) && fs.statSync(path.join(fresh, n)).isFile() && !/\.(so|desktop|sh)$|^chrome|^lib/.test(n));
    if (!exe) throw new Error('The new version has no program in it. Nothing was changed.');
    const old = dir + '.old-' + Date.now();
    fs.renameSync(dir, old);
    try { fs.renameSync(fresh, dir); } catch (e) { try { fs.renameSync(old, dir); } catch { /* the folder is still at .old */ } throw new Error('Could not put the new version in place: ' + e.message); }
    try { fs.rmSync(old, { recursive: true, force: true }); } catch { /* it is only the old copy */ }
    return { dir, exe: path.join(dir, exe), appRun: path.join(dir, 'AppRun') };
  } finally { cleanup(); }
}

/** A package under /usr or /opt cannot be replaced without a password. On a computer that cannot install .deb files, or when that did not work, the app moves to the user's own folder as an AppImage:
 *  from then on it updates itself. */
function moveToUserAppImage({ appImage, home = os.homedir() }) {
  const dir = path.join(home, 'Applications'); fs.mkdirSync(dir, { recursive: true });
  const target = path.join(dir, 'Hearth.AppImage'); replaceFile(target, appImage);
  const apps = path.join(home, '.local', 'share', 'applications'); fs.mkdirSync(apps, { recursive: true });
  fs.writeFileSync(path.join(apps, 'hearth.desktop'), `[Desktop Entry]\nType=Application\nName=Hearth\nComment=Claude Code on your phone and computer\nExec="${target}" %U\nTerminal=false\nCategories=Development;Utility;\nStartupWMClass=Hearth\n`);
  return target;
}

module.exports = { linuxKind, verify, download, replaceFile, swapUnpacked, moveToUserAppImage, writable };
