'use strict';
/* Fetches the Rust front (hearth-relay) of a release and puts it next to the program files. Optional: when anything is off, the relay simply keeps running as Node alone.
   Only talks to github.com/veelvoer/hearth. The file is checked against the checksum of the release and must say it is this version before it is used. */
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { spawnSync } = require('child_process');
const { get } = require('./selfupdate');

const REPO = 'veelvoer/hearth';
const assetName = () => (process.platform === 'linux' && process.arch === 'x64' ? 'hearth-relay-linux-x64' : process.platform === 'linux' && process.arch === 'arm64' ? 'hearth-relay-linux-arm64' : '');

const download = async (url) => { const rs = await get(url, { ms: 60000 }); const chunks = []; for await (const c of rs) chunks.push(c); return Buffer.concat(chunks); };

/** Returns 'installed', or why not (a short sentence). Never throws. */
async function install(tag, relayDir) {
  const name = assetName();
  if (!name) return 'no fast part for this computer';
  if (!/^v\d+\.\d+\.\d+$/.test(tag)) return 'bad version';
  const want = tag.replace(/^v/, '');
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'hearth-front-'));
  try {
    const base = `https://github.com/${REPO}/releases/download/${tag}/`;
    const sums = (await download(base + 'hearth-relay.sha256')).toString('utf8');
    const line = sums.split('\n').find((l) => l.trim().endsWith(' ' + name) || l.trim().endsWith('*' + name));
    if (!line) return 'this release has no ' + name;
    const bin = await download(base + name);
    if (crypto.createHash('sha256').update(bin).digest('hex') !== line.trim().split(/\s+/)[0]) return 'the download did not match its checksum';
    const file = path.join(tmp, 'hearth-relay'); fs.writeFileSync(file, bin, { mode: 0o755 });
    const v = spawnSync(file, ['--version'], { timeout: 5000, encoding: 'utf8' });
    if (v.status !== 0 || String(v.stdout).trim() !== want) return 'the fast part does not run here';
    const dest = path.join(relayDir, 'hearth-relay'), next = dest + '.new';
    fs.copyFileSync(file, next); fs.chmodSync(next, 0o755); fs.renameSync(next, dest);   // an old one that is running keeps running until the restart
    return 'installed';
  } catch (e) { return String((e && e.message) || e).slice(0, 120); } finally { fs.rmSync(tmp, { recursive: true, force: true }); }
}
module.exports = { install, assetName };
if (require.main === module) install(process.argv[2], process.argv[3] || __dirname).then((r) => { console.log(r); process.exit(r === 'installed' ? 0 : 1); });
