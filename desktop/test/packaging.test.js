// A packaged app only holds the files listed in package.json. A file that the program needs but is not listed makes the installed app crash at the start, so check that every local require is packed. Run: node desktop/test/packaging.test.js
const fs = require('fs'), path = require('path');
const root = path.join(__dirname, '..'), pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const globs = pkg.build.files.map((g) => g.replace(/\*\*$/, '').replace(/\/$/, ''));
const packed = (rel) => globs.some((g) => rel === g || rel.startsWith(g + '/'));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const seen = new Set(), queue = ['main.js', 'preload.js', 'updater.js'];
while (queue.length) {
  const rel = queue.pop(); if (seen.has(rel)) continue; seen.add(rel);
  const file = path.join(root, rel); if (!fs.existsSync(file)) { ok(`${rel} exists`, false); continue; }
  ok(`${rel} is packed`, packed(rel));
  for (const m of fs.readFileSync(file, 'utf8').matchAll(/require\(\s*['"](\.[^'"]+)['"]\s*\)/g)) {
    let r = path.normalize(path.join(path.dirname(rel), m[1])); if (!fs.existsSync(path.join(root, r))) r += '.js';
    if (fs.existsSync(path.join(root, r)) && r.endsWith('.js')) queue.push(r); else if (!fs.existsSync(path.join(root, r))) ok(`${rel} requires ${m[1]} which exists`, false);
  }
}
for (const f of fs.readdirSync(path.join(root, 'renderer')).filter((n) => n.endsWith('.html'))) for (const m of fs.readFileSync(path.join(root, 'renderer', f), 'utf8').matchAll(/<script[^>]*\ssrc="([^"]+)"/g)) { if (/^https?:/.test(m[1])) continue; const r = path.normalize(path.join('renderer', m[1])); ok(`${f} loads ${m[1]} and it is packed`, fs.existsSync(path.join(root, r)) && packed(r)); }
console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
