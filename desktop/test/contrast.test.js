// The colors of both apps must be readable: text 4.5:1, borders of inputs and chips 3:1 (WCAG AA), in light and dark. Reads the real palettes from the source. Run: node desktop/test/contrast.test.js
const fs = require('fs'), path = require('path');
const lum = (h) => { const [r, g, b] = [1, 3, 5].map((i) => parseInt(h.replace('#', '').slice(i - 1, i + 1), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4)); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const cr = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
let pass = 0, fail = 0; const ok = (n, v, min) => { const g = v >= min; g ? pass++ : fail++; console.log((g ? 'PASS ' : 'FAIL ') + n + '  ' + v.toFixed(2) + ' (needs ' + min + ')'); };
const css = fs.readFileSync(path.join(__dirname, '..', 'renderer', 'style.css'), 'utf8');
const vars = (block) => Object.fromEntries([...block.matchAll(/--([a-z-]+):\s*(#[0-9A-Fa-f]{6})/g)].map((m) => [m[1], m[2]]));
const light = vars(css.slice(css.indexOf(':root {'), css.indexOf('}', css.indexOf(':root {')))), dark = vars(css.slice(css.indexOf('@media (prefers-color-scheme: dark)'), css.indexOf('}', css.indexOf('@media (prefers-color-scheme: dark)'))));
for (const [name, P] of [['desktop light', light], ['desktop dark', dark]]) {
  ok(`${name}: text on background`, cr(P.ink, P.bg), 4.5); ok(`${name}: muted text on background`, cr(P.muted, P.bg), 4.5); ok(`${name}: muted text on card`, cr(P.muted, P.card), 4.5);
  ok(`${name}: accent text on background`, cr(P.clay, P.bg), 4.5); ok(`${name}: text on accent buttons`, cr(P.onclay, P.clay), 4.5); ok(`${name}: border of inputs and chips on card`, cr(P['line-ctl'], P.card), 3);
}
const kt = fs.readFileSync(path.join(__dirname, '..', '..', 'app', 'src', 'main', 'java', 'dev', 'voer', 'hearth', 'Theme.kt'), 'utf8');
const hex = (s) => '#' + s.toUpperCase().slice(-6);
const kv = (src, k) => { const m = new RegExp('\\b' + k + ' = Color\\(0x([0-9A-Fa-f]{8})\\)').exec(src); return m && hex(m[1]); };
const kcrail = hex(/val Crail = Color\(0x([0-9A-Fa-f]{8})\)/.exec(kt)[1]), kclay = hex(/val Clay = Color\(0x([0-9A-Fa-f]{8})\)/.exec(kt)[1]);
const L = kt.slice(kt.indexOf('private val Light'), kt.indexOf('private val Dark')), D = kt.slice(kt.indexOf('private val Dark'), kt.indexOf('/** App-wide colors'));
for (const [name, src, accent] of [['phone light', L, kcrail], ['phone dark', D, kclay]]) {
  const bg = kv(src, 'background'), card = kv(src, 'surfaceVariant'), ink = kv(src, 'onSurface'), muted = kv(src, 'onSurfaceVariant'), onp = kv(src, 'onPrimary'), ctl = kv(src, 'outlineVariant');
  ok(`${name}: text on background`, cr(ink, bg), 4.5); ok(`${name}: muted text on background`, cr(muted, bg), 4.5); ok(`${name}: muted text on card`, cr(muted, card), 4.5);
  ok(`${name}: accent text on background`, cr(accent, bg), 4.5); ok(`${name}: accent text on card`, cr(accent, card), 4.5); ok(`${name}: text on accent buttons`, cr(onp, accent), 4.5); ok(`${name}: border of inputs and chips on card`, cr(ctl, card), 3);
}
console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
