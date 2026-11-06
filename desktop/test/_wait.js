// Waits until a relay has written its token file (it does that when it starts), then a moment more for it to listen. Tests used a fixed sleep, which failed on a busy machine.
const fs = require('fs');
module.exports = async (file) => {
  for (let i = 0; i < 120; i++) { try { const t = JSON.parse(fs.readFileSync(file, 'utf8')).token; if (t) { await new Promise((r) => setTimeout(r, 900)); return t; } } catch { /* not yet */ } await new Promise((r) => setTimeout(r, 150)); }
  throw new Error('the relay did not start: ' + file);
};
