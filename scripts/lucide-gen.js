// Run: npm pack lucide-static && tar xzf lucide-static-*.tgz, then node scripts/lucide-gen.js package/icons "$(node -p "JSON.stringify(require('./scripts/lucide-map.json').phone)")"
// Turns Lucide SVG files into (a) inner SVG markup for the desktop and (b) one path string per icon for the phone.
const fs = require('fs'), path = require('path');
const dir = process.argv[2];
const attrs = (s) => Object.fromEntries([...s.matchAll(/([a-z0-9-]+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
const n = (v) => Number(v || 0);
function inner(name) { const t = fs.readFileSync(path.join(dir, name + '.svg'), 'utf8'); return [...t.matchAll(/<(path|circle|rect|line|polyline|polygon|ellipse)\b([^>]*?)\/>/g)].map((m) => ({ tag: m[1], a: attrs(m[2]), raw: m[0] })); }
function toD({ tag, a }) {
  if (tag === 'path') return a.d;
  if (tag === 'circle' || tag === 'ellipse') { const rx = n(a.r || a.rx), ry = n(a.r || a.ry), cx = n(a.cx), cy = n(a.cy); return `M${cx - rx} ${cy}a${rx} ${ry} 0 1 0 ${2 * rx} 0a${rx} ${ry} 0 1 0 ${-2 * rx} 0`; }
  if (tag === 'line') return `M${a.x1} ${a.y1}L${a.x2} ${a.y2}`;
  if (tag === 'polyline' || tag === 'polygon') { const p = a.points.trim().split(/[\s,]+/).map(Number); let d = `M${p[0]} ${p[1]}`; for (let i = 2; i < p.length; i += 2) d += `L${p[i]} ${p[i + 1]}`; return d + (tag === 'polygon' ? 'Z' : ''); }
  if (tag === 'rect') { const x = n(a.x), y = n(a.y), w = n(a.width), h = n(a.height); let rx = n(a.rx || a.ry), ry = n(a.ry || a.rx); rx = Math.min(rx, w / 2); ry = Math.min(ry, h / 2);
    if (!rx) return `M${x} ${y}h${w}v${h}h${-w}Z`;
    return `M${x + rx} ${y}h${w - 2 * rx}a${rx} ${ry} 0 0 1 ${rx} ${ry}v${h - 2 * ry}a${rx} ${ry} 0 0 1 ${-rx} ${ry}h${-(w - 2 * rx)}a${rx} ${ry} 0 0 1 ${-rx} ${-ry}v${-(h - 2 * ry)}a${rx} ${ry} 0 0 1 ${rx} ${-ry}Z`; }
  throw new Error(tag);
}
const map = JSON.parse(process.argv[3]);
const out = { desktop: {}, phone: {} };
for (const [k, v] of Object.entries(map)) { const els = inner(v); out.desktop[k] = els.map((e) => e.raw.replace(/\s+/g, ' ').replace(' />', '/>')).join(''); out.phone[k] = els.map(toD).join(' '); }
process.stdout.write(JSON.stringify(out));
