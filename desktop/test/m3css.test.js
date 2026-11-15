// The Material 3 stylesheet must never change the default look: every rule is scoped to the m3 style, except the one that hides the rail icons. Run: node desktop/test/m3css.test.js
const fs = require('fs'), path = require('path');
const css = fs.readFileSync(path.join(__dirname, '..', 'renderer', 'm3.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
// selectors of every top-level rule (also inside @media)
const selectors = []; css.replace(/(?:^|[}{])\s*([^@{}][^{}]*)\{/g, (m, sel) => { selectors.push(sel.trim()); return m; });
const loose = selectors.flatMap((s) => s.split(',').map((x) => x.trim())).filter((s) => s && !s.startsWith(':root[data-style="m3"]') && s !== '#nav .tabs button > .ic' && !/^\d|^from|^to|^@media/.test(s));
ok('1 every rule is scoped to :root[data-style="m3"] (or hides the rail icons)', loose.length === 0, loose.slice(0, 3).join(' | '));
ok('2 the page loads m3.css after style.css', /style\.css"[^>]*><link rel="stylesheet" href="m3\.css"/.test(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'index.html'), 'utf8')));
const app = fs.readFileSync(path.join(__dirname, '..', 'renderer', 'app.js'), 'utf8');
ok('3 the Style choice sets the m3 attribute and turns the frosted glass off', /st\.style === 'material'/.test(app) && /dataset\.style = 'm3'/.test(app) && /classList\.toggle\('noglass', st\.glass !== true \|\| m3\)/.test(app));
console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
