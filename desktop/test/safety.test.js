// Snapshots with undo, backups of chats, and the scan for secrets. Run: node desktop/test/safety.test.js
const fs = require('fs'), os = require('os'), path = require('path');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
const S = require('../relay/safety');
const T = fs.mkdtempSync(path.join(os.tmpdir(), 'hearth-safety-')), dir = path.join(T, 'data'), proj = path.join(T, 'projects', 'app'); fs.mkdirSync(path.join(proj, 'src'), { recursive: true }); fs.mkdirSync(path.join(proj, 'node_modules', 'x'), { recursive: true });
fs.writeFileSync(path.join(proj, 'a.txt'), 'original a'); fs.writeFileSync(path.join(proj, 'c.txt'), 'original c'); fs.writeFileSync(path.join(proj, 'src', 'deep file.js'), 'deep'); fs.writeFileSync(path.join(proj, 'node_modules', 'x', 'big.js'), 'heavy');
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const snap = S.takeSnapshot({ dir, cwd: proj, text: 'make an app' });
ok('1 a snapshot is taken of the project, without the heavy folders', !!snap.id && snap.files === 3, JSON.stringify(snap).slice(0, 120));
fs.writeFileSync(path.join(proj, 'a.txt'), 'CHANGED'); fs.unlinkSync(path.join(proj, 'c.txt')); fs.writeFileSync(path.join(proj, 'new.txt'), 'made by Claude'); fs.mkdirSync(path.join(proj, 'newdir')); fs.writeFileSync(path.join(proj, 'newdir', 'n.txt'), 'x'); fs.writeFileSync(path.join(proj, 'src', 'deep file.js'), 'edited');
const r = S.restoreSnapshot({ dir, cwd: proj, id: snap.id });
ok('2 undo brings back changed and deleted files', r.ok && fs.readFileSync(path.join(proj, 'a.txt'), 'utf8') === 'original a' && fs.readFileSync(path.join(proj, 'c.txt'), 'utf8') === 'original c' && fs.readFileSync(path.join(proj, 'src', 'deep file.js'), 'utf8') === 'deep', JSON.stringify(r));
ok('3 undo removes what was made since, with its empty folders', !fs.existsSync(path.join(proj, 'new.txt')) && !fs.existsSync(path.join(proj, 'newdir')) && r.removed === 2);
ok('4 the heavy folders are left alone', fs.readFileSync(path.join(proj, 'node_modules', 'x', 'big.js'), 'utf8') === 'heavy');
ok('5 the undo itself can be undone (a snapshot of before it was kept)', !!r.undoId && S.listSnapshots({ dir, cwd: proj }).some((s) => s.id === r.undoId));
ok('6 a home folder or the root is never snapshotted', S.takeSnapshot({ dir, cwd: os.homedir() }).skipped && S.takeSnapshot({ dir, cwd: '/' }).skipped);
for (let i = 0; i < 10; i++) { S.takeSnapshot({ dir, cwd: proj }); }
ok('7 only the last 8 snapshots are kept', S.listSnapshots({ dir, cwd: proj }).length === 8);
// backups
const chats = path.join(T, 'claude', 'projects'); fs.mkdirSync(path.join(chats, '-p-a'), { recursive: true }); fs.mkdirSync(path.join(chats, '.hearth-trash'), { recursive: true });
fs.writeFileSync(path.join(chats, '-p-a', 'one.jsonl'), '{"a":1}\n'); fs.writeFileSync(path.join(chats, '.hearth-trash', 'gone.jsonl'), 'trash');
const b = S.createBackup({ dir, chatsDir: chats });
ok('8 a backup of the chats is made', b.ok && S.listBackups(dir).length === 1 && /^chats-\d{14}\.tgz$/.test(b.name));
fs.unlinkSync(path.join(chats, '-p-a', 'one.jsonl')); const rb = S.restoreBackup({ dir, chatsDir: chats, name: b.name });
const listing = require('child_process').spawnSync('tar', ['-tzf', path.join(dir, 'backups', b.name)], { encoding: 'utf8' }).stdout;
ok('9 restoring brings a deleted chat back, and the trash was not part of the backup', rb.ok && fs.existsSync(path.join(chats, '-p-a', 'one.jsonl')) && !/gone\.jsonl/.test(listing));
fs.writeFileSync(path.join(chats, '-p-a', 'one.jsonl'), 'NEWER CONTENT');
S.restoreBackup({ dir, chatsDir: chats, name: b.name });
ok('10 a chat that is newer than the backup is not overwritten', fs.readFileSync(path.join(chats, '-p-a', 'one.jsonl'), 'utf8') === 'NEWER CONTENT');
ok('11 a strange backup name is refused', !!S.restoreBackup({ dir, chatsDir: chats, name: '../../etc/passwd' }).error);
// secrets
const sp = path.join(T, 'projects', 'leaky'); fs.mkdirSync(sp, { recursive: true });
fs.writeFileSync(path.join(sp, '.env'), 'KEY=AKIAABCDEFGHIJKLMNOP\n'); fs.writeFileSync(path.join(sp, 'fine.js'), 'const a = 1;'); fs.mkdirSync(path.join(sp, 'node_modules')); fs.writeFileSync(path.join(sp, 'node_modules', 'k.js'), '-----BEGIN PRIVATE KEY-----');
fs.writeFileSync(path.join(sp, 'tok.txt'), 'ghp_' + 'a'.repeat(36));
const sc = S.scanSecrets({ root: path.join(T, 'projects') });
ok('12 secrets are found with their place and kind, never their value', sc.findings.length === 2 && sc.findings.some((f) => f.file === 'leaky/.env' && /AWS/.test(f.kind)) && sc.findings.some((f) => /GitHub/.test(f.kind)) && !JSON.stringify(sc).includes('AKIAABC'));
ok('13 heavy folders are not searched', !sc.findings.some((f) => /node_modules/.test(f.file)));
fs.rmSync(T, { recursive: true, force: true });
console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
