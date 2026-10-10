'use strict';
/* What the tokens WOULD cost at Anthropic's published API prices (per million tokens). Subscriptions (Pro, Max) are not billed per token,
   so this is the "API-equivalent" value of the work, like the cost line in Claude Code. Prices as of 2026-09; edit here when they change. */
const TABLE = [   // [pattern, input $/M, output $/M, cache-read share of input, label]
  [/fable-5/, 10, 50, 0.1, 'Fable 5'],
  [/opus-5-5/, 4, 20, 0.05, 'Opus 5.5'],
  [/opus-(5|4-8|4-7|4-6|4-5)/, 5, 25, 0.1, 'Opus'],
  [/opus/, 15, 75, 0.1, 'Opus (older)'],
  [/sonnet-5/, 2, 10, 0.1, 'Sonnet 5'],
  [/sonnet/, 3, 15, 0.1, 'Sonnet'],
  [/haiku-4/, 1, 5, 0.1, 'Haiku 4.5'],
  [/haiku/, 0.8, 4, 0.1, 'Haiku'],
];
const CACHE_WRITE = 1.25;   // writing to the 5-minute cache costs 25% more than normal input
function priceOf(model) { const m = String(model || '').toLowerCase(); for (const [re, i, o, cr, label] of TABLE) if (re.test(m)) return { in: i, out: o, cr: cr * i, cw: CACHE_WRITE * i, label, known: true }; return { in: 3, out: 15, cr: 0.3, cw: 3.75, label: model || 'unknown', known: false }; }
/** Dollars for one set of token counts. */
function costOf(model, t) { const p = priceOf(model); return (t.in * p.in + t.out * p.out + t.cr * p.cr + t.cw * p.cw) / 1e6; }
/** A readable name: claude-opus-4-8 -> "Opus 4.8". */
function labelOf(model) { const m = String(model || ''); const x = /(opus|sonnet|haiku|fable|mythos)[- ](\d+)(?:[-.](\d+))?/i.exec(m); return x ? x[1][0].toUpperCase() + x[1].slice(1).toLowerCase() + ' ' + x[2] + (x[3] && x[3].length < 3 ? '.' + x[3] : '') : m.replace(/^claude-/, '') || 'unknown'; }
module.exports = { priceOf, costOf, labelOf };
