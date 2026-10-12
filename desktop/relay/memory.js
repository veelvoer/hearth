'use strict';
/* How much memory the server may use. By default 75% of the memory that is free when it starts (never less than 256 MB).
   Node limits the JavaScript heap with --max-old-space-size, and the limit can only be set when the process starts, so the server
   starts itself again with the right number (see standalone.js). 0 means no limit of our own. */
const fs = require('fs');
const os = require('os');
const MB = 1024 * 1024;
const DEFAULT_PCT = 75, MIN_MB = 256;

/** Memory that can be used right now without swapping (Linux: MemAvailable, elsewhere: what the OS reports free). */
function availableBytes() {
  try { const m = /MemAvailable:\s+(\d+) kB/.exec(fs.readFileSync('/proc/meminfo', 'utf8')); if (m) return Number(m[1]) * 1024; } catch { /* not Linux */ }
  return os.freemem();
}
const pctOf = (cfg) => { const v = cfg && cfg.memoryPct; return v === 0 ? 0 : Number.isFinite(Number(v)) && Number(v) >= 5 && Number(v) <= 95 ? Math.round(Number(v)) : DEFAULT_PCT; };
/** The heap limit in MB for [pct] percent of what is free (null = no limit). */
function limitMb(pct, avail = availableBytes(), total = os.totalmem()) {
  if (!pct) return null;
  return Math.max(MIN_MB, Math.floor(Math.min(avail * pct / 100, total * 0.9) / MB));
}
module.exports = { availableBytes, pctOf, limitMb, DEFAULT_PCT, MIN_MB };
