'use strict';
/* Things that keep a person safe while Claude works: a snapshot of a project before every run (and an undo that puts everything back), a nightly backup of the chats,
   and a scan for secrets that were left in files. No dependency on the server, so it can be tested on its own. Needs tar and find (every Linux and macOS has them). */
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { spawnSync } = require('child_process');

const HEAVY = ['node_modules', '.git', '.venv', 'venv', '__pycache__', '.cache', 'dist', 'build', '.next', 'target', '.uploads', '.gradle', '.idea', '.hearth-trash'];
const MAX_FILES = 20000, KEEP_SNAPSHOTS = 8;
const key = (cwd) => crypto.createHash('sha1').update(path.resolve(cwd)).digest('hex').slice(0, 12);

/** Every ordinary file of a folder (relative paths), skipping the heavy folders that can be rebuilt. null when there are too many. */
function filesOf(cwd, max = MAX_FILES) {
  const args = [cwd, '-xdev', '(', ...HEAVY.flatMap((n, i) => (i ? ['-o', '-name', n] : ['-name', n])), ')', '-prune', '-o', '-type', 'f', '-size', '-5M', '-printf', '%P\\0'];
  const r = spawnSync('find', args, { timeout: 15000, maxBuffer: 128 * 1024 * 1024, encoding: 'buffer' });
  if (r.error || r.status !== 0) return null;
  const list = r.stdout.toString('utf8').split('\0').filter(Boolean);
  return list.length > max ? null : list;
}
const unsafeFolder = (cwd) => { const r = path.resolve(cwd); return r === path.parse(r).root || r === os.homedir() || r.split(path.sep).filter(Boolean).length < 2; };

/** Takes a snapshot of [cwd] into [dir]/snapshots. Returns the snapshot, or { skipped: why }. */
function takeSnapshot({ dir, cwd, label = 'before a run', sid = '', text = '', maxFiles = MAX_FILES }) {
  if (!cwd || unsafeFolder(cwd) || !fs.existsSync(cwd)) return { skipped: 'not a project folder' };
  const files = filesOf(cwd, maxFiles); if (!files) return { skipped: 'too many files' };
  const id = String(Date.now()), where = path.join(dir, 'snapshots', key(cwd)); fs.mkdirSync(where, { recursive: true, mode: 0o700 });
  const list = path.join(where, id + '.list'), out = path.join(where, id + '.tgz');
  fs.writeFileSync(list, files.join('\0'), { mode: 0o600 });
  const r = spawnSync('tar', ['-czf', out, '-C', cwd, '--null', '-T', list], { timeout: 120000 });
  try { fs.unlinkSync(list); } catch { /* temp */ }
  if (r.status !== 0 && r.status !== 1) { try { fs.unlinkSync(out); } catch { /* none */ } return { skipped: 'tar failed' }; }   // 1 = a file changed while it was read: fine
  const meta = { id, cwd: path.resolve(cwd), sid, label, text: String(text).slice(0, 200), at: Date.now(), files: files.length, bytes: fs.statSync(out).size };
  fs.writeFileSync(path.join(where, id + '.json'), JSON.stringify(meta), { mode: 0o600 });
  const all = listSnapshots({ dir, cwd }); for (const old of all.slice(KEEP_SNAPSHOTS)) for (const ext of ['.tgz', '.json']) { try { fs.unlinkSync(path.join(where, old.id + ext)); } catch { /* gone */ } }
  return meta;
}
function listSnapshots({ dir, cwd }) {
  const where = path.join(dir, 'snapshots', key(cwd)); let names = []; try { names = fs.readdirSync(where); } catch { return []; }
  return names.filter((n) => n.endsWith('.json')).map((n) => { try { return JSON.parse(fs.readFileSync(path.join(where, n), 'utf8')); } catch { return null; } }).filter(Boolean).sort((a, b) => b.at - a.at);
}
/** Puts [cwd] back the way it was in the snapshot: files that were changed or deleted come back, files that were made since are removed. A snapshot of the present is taken first, so the undo can be undone. */
function restoreSnapshot({ dir, cwd, id }) {
  const snap = listSnapshots({ dir, cwd }).find((s) => s.id === String(id)); if (!snap) return { error: 'That snapshot is gone.' };
  const file = path.join(dir, 'snapshots', key(cwd), snap.id + '.tgz'); if (!fs.existsSync(file)) return { error: 'That snapshot is gone.' };
  const before = takeSnapshot({ dir, cwd, label: 'before an undo', text: 'before going back to ' + new Date(snap.at).toLocaleString() });
  const lt = spawnSync('tar', ['-tzf', file], { timeout: 60000, maxBuffer: 128 * 1024 * 1024, encoding: 'utf8' }); if (lt.status !== 0) return { error: 'The snapshot could not be read.' };
  const keep = new Set(lt.stdout.split('\n').filter(Boolean).map((p) => p.replace(/^\.\//, '')));
  const now = filesOf(cwd, Infinity) || []; let removed = 0;
  for (const f of now) if (!keep.has(f)) { try { fs.unlinkSync(path.join(cwd, f)); removed++; } catch { /* in use */ } }
  const dirs = [...new Set(now.map((f) => path.dirname(f)).filter((d) => d !== '.'))].sort((a, b) => b.length - a.length);
  for (const d of dirs) { try { fs.rmdirSync(path.join(cwd, d)); } catch { /* not empty */ } }
  const x = spawnSync('tar', ['-xzf', file, '-C', cwd], { timeout: 120000 }); if (x.status !== 0) return { error: 'Putting the files back failed. A snapshot of how it was just now is kept.' };
  return { ok: true, restored: keep.size, removed, undoId: before && before.id };
}

// ── backups of the chats ──
const stamp = () => new Date().toISOString().replace(/[-:T]/g, '').slice(0, 14);
function listBackups(dir) { let names = []; try { names = fs.readdirSync(path.join(dir, 'backups')); } catch { return []; } return names.filter((n) => /^chats-\d{14}\.tgz$/.test(n)).sort().reverse().map((n) => { const st = fs.statSync(path.join(dir, 'backups', n)); return { name: n, bytes: st.size, at: st.mtimeMs }; }); }
function createBackup({ dir, chatsDir, keep = 7 }) {
  if (!fs.existsSync(chatsDir)) return { error: 'There are no chats to back up yet.' };
  const du = spawnSync('du', ['-sk', chatsDir], { timeout: 20000, encoding: 'utf8' }); if (du.status === 0 && Number(String(du.stdout).split(/\s/)[0]) > 2 * 1024 * 1024) return { error: 'The chats are bigger than 2 GB: too big for a backup here.' };
  const where = path.join(dir, 'backups'); fs.mkdirSync(where, { recursive: true, mode: 0o700 });
  const name = 'chats-' + stamp() + '.tgz', out = path.join(where, name);
  const r = spawnSync('tar', ['-czf', out, '-C', chatsDir, '--exclude=.hearth-trash', '.'], { timeout: 600000 });
  if (r.status !== 0 && r.status !== 1) { try { fs.unlinkSync(out); } catch { /* none */ } return { error: 'The backup failed.' }; }
  fs.chmodSync(out, 0o600);
  for (const old of listBackups(dir).slice(keep)) { try { fs.unlinkSync(path.join(where, old.name)); } catch { /* gone */ } }
  return { ok: true, name, bytes: fs.statSync(out).size };
}
/** Puts the chats of a backup back. Chats that are newer than the backup are not overwritten. */
function restoreBackup({ dir, chatsDir, name }) {
  if (!/^chats-\d{14}\.tgz$/.test(String(name))) return { error: 'Unknown backup.' };
  const file = path.join(dir, 'backups', name); if (!fs.existsSync(file)) return { error: 'That backup is gone.' };
  fs.mkdirSync(chatsDir, { recursive: true });
  const r = spawnSync('tar', ['-xzf', file, '-C', chatsDir, '--keep-newer-files'], { timeout: 600000 });
  return r.status === 0 || r.status === 2 ? { ok: true } : { error: 'Restoring failed.' };   // 2: it kept some newer files, as meant
}

// ── secrets left in files ──
const PATTERNS = [['an AWS access key', /AKIA[0-9A-Z]{16}/], ['a private key', /-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----/], ['a GitHub token', /\bgh[pousr]_[A-Za-z0-9]{36,}\b/], ['an Anthropic key', /\bsk-ant-[A-Za-z0-9_-]{20,}/],
  ['a Slack token', /\bxox[baprs]-[A-Za-z0-9-]{10,}/], ['a Google API key', /\bAIza[0-9A-Za-z_-]{35}\b/], ['an OpenAI key', /\bsk-[A-Za-z0-9]{32,}\b/]];
/** Looks through the recently changed small text files of a folder. Never returns the secret itself, only where it is and what kind. */
function scanSecrets({ root, days = 30, maxFiles = 2500 }) {
  const out = [], cutoff = Date.now() - days * 864e5; let seen = 0;
  const walk = (d, depth) => {
    if (depth > 8 || seen >= maxFiles) return; let es; try { es = fs.readdirSync(d, { withFileTypes: true }); } catch { return; }
    for (const e of es) {
      if (seen >= maxFiles) return; const p = path.join(d, e.name);
      if (e.isDirectory()) { if (!HEAVY.includes(e.name)) walk(p, depth + 1); continue; }
      if (!e.isFile()) continue; let st; try { st = fs.statSync(p); } catch { continue; }
      if (st.size > 262144 || st.mtimeMs < cutoff) continue; seen++;
      let text; try { text = fs.readFileSync(p, 'utf8'); } catch { continue; } if (text.includes('\0')) continue;
      for (const [kind, re] of PATTERNS) if (re.test(text)) { out.push({ file: path.relative(root, p), kind }); break; }
    }
  };
  walk(root, 0); return { findings: out, scanned: seen };
}
module.exports = { takeSnapshot, listSnapshots, restoreSnapshot, createBackup, listBackups, restoreBackup, scanSecrets, filesOf, HEAVY };
