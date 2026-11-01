// The whole update flow of the installed app (download → install → restart request) for each way of being installed, with a fake GitHub and fake AppImages. Run: node desktop/test/updater-flow.test.js
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform !== 'linux') { console.log('skipped: Linux only'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-uflow-'));
const gh = require('../relay/selfupdate'), updater = require('../updater');
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const fake = (v) => `#!/bin/sh
if [ "$1" = "--appimage-extract" ]; then mkdir -p squashfs-root/resources; printf "#!/bin/sh\\n" > squashfs-root/AppRun; printf "#!/bin/sh\\n" > squashfs-root/hearth; chmod +x squashfs-root/AppRun squashfs-root/hearth; echo ${v} > squashfs-root/VERSION; echo x > squashfs-root/resources/app.asar; exit 0; fi
echo ${v}
`;
const body = fake('99.0.0');
const srv = http.createServer((q, s) => { s.writeHead(200, { 'Content-Length': Buffer.byteLength(body) }); s.end(body); }).listen(0, '127.0.0.1');
(async () => {
  await new Promise((r) => srv.once('listening', r));
  const url = 'http://127.0.0.1:' + srv.address().port + '/Hearth-99.0.0-linux-x86_64.AppImage';
  gh.latest = async () => ({ tag: 'v99.0.0', version: '99.0.0', notes: '', assets: [{ name: 'Hearth-99.0.0-linux-x86_64.AppImage', url, size: Buffer.byteLength(body) }, { name: 'Hearth-99.0.0-linux-amd64.deb', url, size: 1 }] });
  gh.get = (u) => new Promise((res, rej) => http.get(u, (rs) => res(rs)).on('error', rej));
  const sent = []; let relaunched;
  const deskDir = path.join(T, 'asar-desktop'); fs.mkdirSync(deskDir);   // no main.js here: behaves like the packaged app
  const mk = () => updater.create({ root: T, desktopDir: deskDir, getState: () => ({}), save() {}, send: (ch, d) => sent.push(d), serverBusy: async () => false, localBusy: async () => false,
    relaunch: (p) => { relaunched = p || 'default'; }, relayBuild: async () => undefined, version: () => '1.0.0', relayStatus: async () => undefined, relayPost: async () => ({}), openPath() {}, openExternal() {}, notify() {}, windowHidden: () => false, hasDpkg: false });
  const run = async () => { sent.length = 0; relaunched = undefined; await mk().run({ desktop: true }); await new Promise((r) => setTimeout(r, 1600)); return sent.filter((x) => x.step === 'desktop').map((x) => x.text + '|' + x.state); };
  const keep = { APPIMAGE: process.env.APPIMAGE, APPDIR: process.env.APPDIR, HOME: process.env.HOME };
  // 1 AppImage file
  const ai = path.join(T, 'Hearth.AppImage'); fs.writeFileSync(ai, fake('1.0.0'), { mode: 0o755 });
  process.env.APPIMAGE = ai; delete process.env.APPDIR; let log = await run();
  ok('1 AppImage file: replaced, and a restart from the same file is requested', /99\.0\.0/.test(fs.readFileSync(ai, 'utf8')) && relaunched === ai && !fs.existsSync(ai + '.new') && !fs.existsSync(ai + '.new.part'), log.join(' / '));
  // 2 unpacked AppImage, started through AppRun
  delete process.env.APPIMAGE; const ex = path.join(T, 'squashfs-root'); fs.mkdirSync(path.join(ex, 'resources'), { recursive: true }); fs.writeFileSync(path.join(ex, 'AppRun'), '#!/bin/sh\n', { mode: 0o755 }); fs.writeFileSync(path.join(ex, 'hearth'), '#!/bin/sh\n', { mode: 0o755 }); fs.writeFileSync(path.join(ex, 'VERSION'), '1.0.0'); fs.writeFileSync(path.join(ex, 'resources', 'app.asar'), 'x');
  process.env.APPDIR = ex; log = await run();
  ok('2 unpacked AppImage: the folder holds the new version and the restart goes through AppRun', fs.readFileSync(path.join(ex, 'VERSION'), 'utf8').trim() === '99.0.0' && relaunched === path.join(ex, 'AppRun'), log.join(' / ') + ' → ' + relaunched);
  ok('3 nothing is left behind next to it', fs.readdirSync(T).filter((n) => /\.old-|hearth-update-/.test(n)).length === 0);
  // 3 a computer with neither (a package, not Debian): the app moves to the user's own folder
  delete process.env.APPDIR; const home = path.join(T, 'home'); fs.mkdirSync(home); process.env.HOME = home; log = await run();
  const moved = path.join(home, 'Applications', 'Hearth.AppImage');
  ok('4 package install on a computer without dpkg: moves to ~/Applications and restarts from there', fs.existsSync(moved) && relaunched === moved && fs.existsSync(path.join(home, '.local', 'share', 'applications', 'hearth.desktop')) && /updates itself|runs from/.test(log.join(' ')), log.join(' / '));
  // 4 failures leave the old app working and say what to do
  process.env.HOME = keep.HOME; fs.writeFileSync(path.join(ex, 'VERSION'), '1.0.0');
  const broken = fake('98').replace('mkdir -p squashfs-root/resources;', 'mkdir -p squashfs-root/resources; rm -rf squashfs-root/resources;');
  const bsrv = http.createServer((q, s) => { s.writeHead(200, { 'Content-Length': Buffer.byteLength(broken) }); s.end(broken); }).listen(0, '127.0.0.1'); await new Promise((r) => bsrv.once('listening', r));
  gh.latest = async () => ({ tag: 'v99.0.0', version: '99.0.0', notes: '', assets: [{ name: 'Hearth-99.0.0-linux-x86_64.AppImage', url: 'http://127.0.0.1:' + bsrv.address().port + '/x', size: Buffer.byteLength(broken) }] });
  process.env.APPDIR = ex; log = await run();
  ok('5 a broken new version: the old app is untouched, nothing restarts, and the message says what to do', fs.readFileSync(path.join(ex, 'VERSION'), 'utf8').trim() === '1.0.0' && relaunched === undefined && /still works/.test(log.join(' ')) && /download it yourself/.test(log.join(' ')) && /\|fail/.test(log.join(' ')), log.join(' / '));
  bsrv.close(); srv.close(); Object.assign(process.env, keep); if (!keep.APPIMAGE) delete process.env.APPIMAGE; if (!keep.APPDIR) delete process.env.APPDIR;
  fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
