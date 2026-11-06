// A chat file that the server's user cannot read (made by a run with root rights) is noticed, so it can be given back. Run: node desktop/test/rootfiles.test.js
const fs = require('fs'), os = require('os'), path = require('path');
if (process.platform === 'win32' || (process.getuid && process.getuid() === 0)) { console.log('skipped: needs a normal Linux user'); process.exit(0); }
const T = fs.mkdtempSync(path.join(os.tmpdir(), 'hearth-rootfiles-')); process.env.HOME = T;
const dir = path.join(T, '.claude', 'projects', '-proj-a'); fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(path.join(dir, 'ok.jsonl'), JSON.stringify({ type: 'user', cwd: '/proj/a', message: { content: 'hi' } }) + '\n');
fs.writeFileSync(path.join(dir, 'locked.jsonl'), 'x\n'); fs.chmodSync(path.join(dir, 'locked.jsonl'), 0);
const lockedDir = path.join(T, '.claude', 'projects', '-proj-b'); fs.mkdirSync(lockedDir); fs.writeFileSync(path.join(lockedDir, 'in.jsonl'), 'x\n'); fs.chmodSync(lockedDir, 0);
const core = require('../relay/core');
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const files = core.listFiles();
ok('1 a readable chat is listed, the locked ones are skipped', files.length === 1 && files[0].id === 'ok');
ok('2 the locked file and the locked folder are both counted', core.deniedCount() === 2, String(core.deniedCount()));
fs.chmodSync(lockedDir, 0o755); fs.chmodSync(path.join(dir, 'locked.jsonl'), 0o644);
ok('3 once readable again they are listed', core.listFiles().length === 3);
fs.rmSync(T, { recursive: true, force: true });
console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
