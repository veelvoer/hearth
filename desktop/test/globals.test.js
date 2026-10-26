// The pages of the app load their scripts into one shared scope: two scripts (or one script twice) that declare the same function or constant silently replace each other. Run: node desktop/test/globals.test.js
const fs = require('fs'), path = require('path');
const dir = path.join(__dirname, '..', 'renderer'); let bad = 0;
for (const page of fs.readdirSync(dir).filter((f) => f.endsWith('.html'))) {
  const html = fs.readFileSync(path.join(dir, page), 'utf8'), seen = new Map();
  for (const m of html.matchAll(/<script[^>]*\ssrc="([^"]+)"/g)) {
    const f = path.join(dir, m[1]); if (!fs.existsSync(f) || /^https?:/.test(m[1])) continue;
    const src = fs.readFileSync(f, 'utf8');
    for (const d of src.matchAll(/^(?:async\s+)?function\s+([A-Za-z0-9_$]+)\s*\(|^(?:const|let|var)\s+([A-Za-z0-9_$]+)\s*=/gm)) {
      const name = d[1] || d[2], where = path.basename(m[1]);
      if (seen.has(name)) { bad++; console.log(`FAIL ${page}: "${name}" is declared in ${seen.get(name)} and again in ${where}`); } else seen.set(name, where);
    }
  }
}
console.log(bad ? `\n0 passed, ${bad} failed` : 'PASS no name is declared twice in any page\n\n1 passed, 0 failed'); process.exit(bad ? 1 : 0);
