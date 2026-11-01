// Updating the app in every way it can be installed on Linux: AppImage file, unpacked AppImage folder, package. Uses a fake AppImage (a script that unpacks like one). Run: node desktop/test/installer.test.js
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http'), crypto = require('crypto');
const inst = require('../installer');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-inst-'));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const fakeAppImage = (file, { version = '2', broken = false } = {}) => fs.writeFileSync(file, `#!/bin/sh
if [ "$1" = "--appimage-extract" ]; then
  mkdir -p squashfs-root/resources
  ${broken ? '' : 'printf "#!/bin/sh\\necho apprun\\n" > squashfs-root/AppRun; chmod +x squashfs-root/AppRun'}
  printf "#!/bin/sh\\necho hearth ${version}\\n" > squashfs-root/hearth; chmod +x squashfs-root/hearth
  echo ${version} > squashfs-root/VERSION; echo x > squashfs-root/resources/app.asar; echo lib > squashfs-root/libffmpeg.so
  exit 0
fi
echo running appimage ${version}
`, { mode: 0o755 });
const unpacked = (dir, v = '1') => { fs.mkdirSync(path.join(dir, 'resources'), { recursive: true }); fs.writeFileSync(path.join(dir, 'AppRun'), '#!/bin/sh\n', { mode: 0o755 }); fs.writeFileSync(path.join(dir, 'hearth'), '#!/bin/sh\necho hearth ' + v + '\n', { mode: 0o755 }); fs.writeFileSync(path.join(dir, 'VERSION'), v); fs.writeFileSync(path.join(dir, 'resources', 'app.asar'), 'x'); };
(async () => {
  // 1 detecting how the app is installed
  const ai = path.join(T, 'Hearth.AppImage'); fakeAppImage(ai, { version: '1' });
  ok('1 an AppImage is recognised by APPIMAGE', inst.linuxKind({ env: { APPIMAGE: ai }, execPath: '/tmp/.mount_x/hearth' }).kind === 'appimage');
  const ex = path.join(T, 'squashfs-root'); unpacked(ex);
  ok('2 an unpacked AppImage is recognised, started through the binary', inst.linuxKind({ env: {}, execPath: path.join(ex, 'hearth') }).kind === 'extracted' && !inst.linuxKind({ env: {}, execPath: path.join(ex, 'hearth') }).viaAppRun);
  const k3 = inst.linuxKind({ env: { APPDIR: ex }, execPath: path.join(ex, 'hearth') });
  ok('3 an unpacked AppImage is recognised, started through AppRun', k3.kind === 'extracted' && k3.viaAppRun && k3.dir === ex);
  ok('4 a package under /opt is recognised', inst.linuxKind({ env: {}, execPath: '/opt/Hearth/hearth' }).kind === 'system' && inst.linuxKind({ env: {}, execPath: '/usr/lib/hearth/hearth' }).kind === 'system');
  ok('5 an unknown place is not mistaken for something else', inst.linuxKind({ env: {}, execPath: path.join(T, 'nowhere', 'hearth') }).kind === 'unknown');

  // 2 unpacked folder: swap, and undo on failure
  const fresh = path.join(T, 'new.AppImage'); fakeAppImage(fresh, { version: '2' });
  const r = await inst.swapUnpacked({ dir: ex, appImage: fresh });
  ok('6 the unpacked folder now holds the new version, at the same place', fs.readFileSync(path.join(ex, 'VERSION'), 'utf8').trim() === '2' && r.dir === ex && fs.existsSync(r.exe) && (fs.statSync(r.exe).mode & 0o111) && fs.existsSync(r.appRun));
  ok('7 no temporary folders or old copies are left behind', fs.readdirSync(T).filter((n) => /hearth-update|\.old-/.test(n)).length === 0, fs.readdirSync(T).join(' '));
  const bad = path.join(T, 'bad.AppImage'); fakeAppImage(bad, { version: '3', broken: true });
  let err = ''; try { await inst.swapUnpacked({ dir: ex, appImage: bad }); } catch (e) { err = e.message; }
  ok('8 an incomplete new version is refused and the old one is untouched', /incomplete/.test(err) && fs.readFileSync(path.join(ex, 'VERSION'), 'utf8').trim() === '2' && fs.readdirSync(T).filter((n) => /hearth-update|\.old-/.test(n)).length === 0, err);
  const nope = path.join(T, 'nope.AppImage'); fs.writeFileSync(nope, '#!/bin/sh\necho no >&2\nexit 3\n', { mode: 0o755 });
  err = ''; try { await inst.swapUnpacked({ dir: ex, appImage: nope }); } catch (e) { err = e.message; }
  ok('9 an AppImage that cannot unpack is refused and the old one is untouched', /Could not unpack/.test(err) && fs.readFileSync(path.join(ex, 'VERSION'), 'utf8').trim() === '2');
  const ro = path.join(T, 'ro', 'app'); unpacked(ro); fs.chmodSync(path.join(T, 'ro'), 0o555);
  err = ''; let code = ''; try { await inst.swapUnpacked({ dir: ro, appImage: fresh }); } catch (e) { err = e.message; code = e.code; } fs.chmodSync(path.join(T, 'ro'), 0o755);
  ok('10 a read-only place says so with a code the updater can act on', code === 'EACCES' || process.getuid() === 0, err);

  // 3 AppImage file
  const target = path.join(T, 'Hearth2.AppImage'); fakeAppImage(target, { version: '1' }); const nf = target + '.new'; fakeAppImage(nf, { version: '9' });
  inst.replaceFile(target, nf);
  ok('11 an AppImage file is replaced in one step and stays runnable', /9/.test(fs.readFileSync(target, 'utf8')) && !fs.existsSync(nf) && (fs.statSync(target).mode & 0o111));

  // 4 moving a package install to the user's own folder
  const home = path.join(T, 'home'); fs.mkdirSync(home); const src = path.join(T, 'src.AppImage'); fakeAppImage(src, { version: '5' });
  const moved = inst.moveToUserAppImage({ appImage: src, home });
  const desk = fs.readFileSync(path.join(home, '.local', 'share', 'applications', 'hearth.desktop'), 'utf8');
  ok('12 the app moves to ~/Applications with a launcher entry', moved === path.join(home, 'Applications', 'Hearth.AppImage') && fs.existsSync(moved) && /Exec="[^"]*Hearth.AppImage"/.test(desk) && /Name=Hearth/.test(desk));

  // 5 downloading: retries, size and checksum
  let hits = 0; const body = crypto.randomBytes(200000), digest = 'sha256:' + crypto.createHash('sha256').update(body).digest('hex');
  const srv = http.createServer((q, s) => { hits++; if (q.url === '/flaky' && hits <= 2) { s.destroy(); return; } if (q.url === '/short') { s.writeHead(200, { 'Content-Length': String(body.length) }); s.end(body.subarray(0, 1000)); return; } s.writeHead(200, { 'Content-Length': String(body.length) }); s.end(body); }).listen(0, '127.0.0.1'); await new Promise((r) => srv.once('listening', r));
  const base = 'http://127.0.0.1:' + srv.address().port, get = (url) => new Promise((res, rej) => http.get(url, (rs) => (rs.statusCode === 200 ? res(rs) : rej(new Error('HTTP ' + rs.statusCode)))).on('error', rej));
  const f1 = path.join(T, 'dl1.bin'); hits = 0;
  await inst.download({ get, asset: { url: base + '/flaky', size: body.length, digest }, file: f1, wait: 10 });
  ok('13 a flaky connection is retried and the file arrives complete', hits === 3 && fs.statSync(f1).size === body.length && !fs.existsSync(f1 + '.part'));
  err = ''; try { await inst.download({ get, asset: { url: base + '/ok', size: body.length, digest: 'sha256:' + '0'.repeat(64) }, file: path.join(T, 'dl2.bin'), tries: 2, wait: 10 }); } catch (e) { err = e.message; }
  ok('14 a download with the wrong checksum is refused and leaves nothing behind', /checksum/.test(err) && !fs.existsSync(path.join(T, 'dl2.bin')) && !fs.existsSync(path.join(T, 'dl2.bin.part')));
  err = ''; try { await inst.download({ get, asset: { url: base + '/short', size: body.length }, file: path.join(T, 'dl3.bin'), tries: 2, wait: 10 }); } catch (e) { err = e.message; }
  ok('15 a cut-off download is refused, in plain words', /interrupted|incomplete/.test(err) && !fs.existsSync(path.join(T, 'dl3.bin')), err);
  srv.close();

  // 6 the real thing, when a real AppImage is at hand (not in CI)
  const real = process.env.HEARTH_REAL_APPIMAGE;
  if (real && fs.existsSync(real)) {
    const rex = path.join(T, 'real', 'squashfs-root'); fs.mkdirSync(path.dirname(rex), { recursive: true }); unpacked(rex, 'old');
    const rr = await inst.swapUnpacked({ dir: rex, appImage: real, env: { ...process.env, ELECTRON_RUN_AS_NODE: undefined } });
    ok('16 a real AppImage unpacks into the place of the old folder', fs.existsSync(rr.exe) && fs.existsSync(path.join(rex, 'resources', 'app.asar')) && !fs.existsSync(path.join(rex, 'VERSION')), rr.exe);
  } else console.log('SKIP 16 real AppImage (set HEARTH_REAL_APPIMAGE to try it)');
  fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
