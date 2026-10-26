'use strict';
/* A fingerprint of the relay's own code. The relay reports it, and the desktop app compares it with the code in the project
   folder to know whether a computer or server is really running the latest version. Same result on every machine. */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

function sigOfDir(dir) {
  const h = crypto.createHash('sha1');
  let names = [];
  try { names = fs.readdirSync(dir).filter((n) => n.endsWith('.js') && n !== 'embedded.js' && !n.includes('conflict')).sort(); } catch { names = []; }   // exactly what gets installed (the same files selfupdate.js and deploy.sh copy)
  for (const n of names) { h.update(n + '\0'); try { h.update(fs.readFileSync(path.join(dir, n))); } catch { /* vanished */ } }
  return h.digest('hex').slice(0, 12);
}
module.exports = { sigOfDir };
