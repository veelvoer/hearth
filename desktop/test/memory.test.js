// Memory share for the server. Run: node desktop/test/memory.test.js
const path = require('path');
const m = require(path.join(__dirname, '..', 'relay', 'memory.js'));
let ok = true; const t = (n, c) => { if (!c) ok = false; console.log((c ? 'PASS ' : 'FAIL ') + n); };
const GB = 1024 ** 3;
t('default is 75%', m.pctOf({}) === 75 && m.pctOf(null) === 75);
t('0 means no limit', m.pctOf({ memoryPct: 0 }) === 0 && m.limitMb(0) === null);
t('out of range falls back', m.pctOf({ memoryPct: 200 }) === 75 && m.pctOf({ memoryPct: 1 }) === 75);
t('75% of 4 GB free = 3072 MB', m.limitMb(75, 4 * GB, 8 * GB) === 3072);
t('never more than 90% of all memory', m.limitMb(95, 20 * GB, 8 * GB) === Math.floor(8 * GB * 0.9 / 1048576));
t('never below 256 MB', m.limitMb(75, 100 * 1048576, 8 * GB) === 256);
process.exit(ok ? 0 : 1);
