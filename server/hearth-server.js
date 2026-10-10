#!/usr/bin/env node
'use strict';
/* Hearth server installer. A friendly text screen (TUI) that sets up everything on a server.
     node hearth-server.js            install (asks simple questions)
     hearth-server code               show a fresh code to connect a phone or computer
     hearth-server status | update | uninstall
   Options for scripts: --yes --mode direct|docker --address auto|none|<domain> --name <text> --prefix <dir> --dry-run
   No packages needed: this file only uses what comes with Node.js. */
const fs = require('fs');
const os = require('os');
const path = require('path');
const http = require('http');
const https = require('https');
const readline = require('readline');
const { spawn, spawnSync } = require('child_process');

const REPO = path.resolve(__dirname, '..');
const RELAY_SRC = path.join(REPO, 'desktop', 'relay');
const RELAY_FILES = (() => { try { return fs.readdirSync(RELAY_SRC).filter((f) => f.endsWith('.js') && f !== 'embedded.js'); } catch { return []; } })();   // the program files of the server
const VERSION = (() => { for (const p of [path.join(RELAY_SRC, 'version.js'), path.join(__dirname, '..', 'relay', 'version.js')]) { try { return require(p); } catch { /* next */ } } return '1.0.0'; })();   // from the repository, or from the installed copy
let PORT = 47601;

// ───────────────────────── arguments ─────────────────────────
const argv = process.argv.slice(2);
const flag = (k) => argv.includes('--' + k);
const opt = (k, d) => { const i = argv.indexOf('--' + k); return i >= 0 && argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[i + 1] : d; };
const VALUED = ['mode', 'address', 'name', 'prefix', 'instance'];
const cmd = argv.find((a, i) => !a.startsWith('--') && !(i > 0 && argv[i - 1].startsWith('--') && VALUED.includes(argv[i - 1].slice(2)))) || 'install';
const YES = flag('yes'), DRY = flag('dry-run'), PREFIX = opt('prefix', '');   // PREFIX: write into a folder instead of the real system (for tests)
const OPT = PREFIX ? path.join(PREFIX, 'opt') : '/opt';
// One server can hold several Hearth installs (one per person). "hearth" is the first; later people get "hearth-<name>".
let INSTANCE = 'hearth', INSTALL_DIR = path.join(OPT, INSTANCE);
const binOf = (i) => (i === 'hearth' ? 'hearth-server' : i + '-server');
const caddyFileOf = (i) => sysPath('/etc/caddy/' + i + '.caddy'), caddyDirOf = (i) => sysPath('/etc/caddy/' + i + '.d');
function findInstances() {
  const out = [];
  try { for (const d of fs.readdirSync(OPT)) if (/^hearth(-[a-z0-9]+)?$/.test(d)) { try { out.push({ instance: d, meta: JSON.parse(fs.readFileSync(path.join(OPT, d, 'install.json'), 'utf8')) }); } catch { /* not an install */ } } } catch { /* none */ }
  return out;
}
const setInstance = (i) => { INSTANCE = i; INSTALL_DIR = path.join(OPT, i); };
const portFree = (p) => new Promise((res) => { const s = require('net').createServer(); s.once('error', () => res(false)); s.listen(p, '127.0.0.1', () => s.close(() => res(true))); });

// ───────────────────────── looks ─────────────────────────
const tty = process.stdout.isTTY && !process.env.NO_COLOR;
const c = (code, s) => (tty ? `\x1b[${code}m${s}\x1b[0m` : s);
const bold = (s) => c('1', s), dim = (s) => c('2', s), ember = (s) => c('38;5;208', s), green = (s) => c('32', s), red = (s) => c('31', s), yellow = (s) => c('33', s);
const W = Math.min(process.stdout.columns || 80, 78);
const wrap = (text, indent = '  ', width = W - 4) => text.split('\n').map((para) => {
  const words = para.split(' '), lines = []; let cur = '';
  for (const w of words) { if ((cur + ' ' + w).trim().length > width) { lines.push(cur); cur = w; } else cur = (cur + ' ' + w).trim(); }
  lines.push(cur); return lines.map((l) => indent + l).join('\n');
}).join('\n');
const say = (t = '') => console.log(wrap(t));
const gap = () => console.log();
const item = (n, text) => { const lines = wrap(text, '', W - 9).split('\n'); console.log('  ' + ember(n + '.') + '  ' + lines[0]); lines.slice(1).forEach((l) => console.log('      ' + l)); };
const FLAME = ['      (  )', '     (    )', '    (  /\\  )', '    ( /  \\ )', '     \\____/', '  ____||____'];
function banner() {
  console.log(); FLAME.forEach((l) => console.log(ember(l)));
  console.log(bold('  Hearth server') + dim('  ·  version ' + VERSION)); gap();
}
function step(n, total, title) { gap(); console.log(ember(`  Step ${n} of ${total}`) + bold('  ' + title)); console.log(dim('  ' + '─'.repeat(Math.min(60, W - 4)))); gap(); }
const ok = (t) => console.log('  ' + green('✓ ') + t);
const warn = (t) => console.log('  ' + yellow('! ') + t);
const bad = (t) => console.log('  ' + red('✗ ') + t);

// ───────────────────────── questions ─────────────────────────
let rl = null;
const input = () => { if (!rl) rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: !!process.stdin.isTTY }); return rl; };
const ask = (q, def = '') => new Promise((res) => {
  if (YES) return res(def);
  input().question('  ' + bold(q) + (def ? dim(` (${def})`) : '') + ' ' + ember('› '), (a) => res(a.trim() || def));
});
async function choose(q, options, def = 0) {
  say(bold(q)); gap();
  options.forEach((o, i) => { console.log('  ' + ember(`${i + 1}`) + '  ' + bold(o.label) + (i === def ? dim('   ← the easy choice') : '')); if (o.hint) console.log(dim(wrap(o.hint, '     ', W - 8))); });
  gap();
  if (YES) return def;
  for (;;) {
    const a = await ask('Type a number and press Enter', String(def + 1));
    const n = Number(a); if (n >= 1 && n <= options.length) return n - 1;
    bad('Please type one of the numbers above.');
  }
}
async function confirm(q, def = true) {
  if (YES) return def;
  for (;;) { const a = (await ask(q + (def ? ' [Y/n]' : ' [y/N]'), '')).toLowerCase(); if (!a) return def; if (/^y/.test(a)) return true; if (/^n/.test(a)) return false; }
}
async function pause(msg = 'Press Enter to continue') { if (!YES) await ask(msg, ''); }

// ───────────────────────── running things ─────────────────────────
const isRoot = process.getuid && process.getuid() === 0;
const have = (bin) => spawnSync('sh', ['-c', `command -v ${bin}`], { encoding: 'utf8' }).status === 0;
const sudoPrefix = () => (isRoot || PREFIX ? [] : ['sudo', '-n']);
const nodePath = () => { const r = spawnSync('sh', ['-c', 'command -v node'], { encoding: 'utf8' }); return (r.stdout || '').trim() || process.execPath; };
const frames = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏';
/** Runs a command with a little spinner. Resolves {ok, out}; never throws. */
function run(label, command, args = [], o = {}) {
  return new Promise((resolve) => {
    if (DRY) { console.log('  ' + dim('[dry run] ' + [command, ...args].join(' '))); return resolve({ ok: true, out: '' }); }
    const full = o.sudo ? [...sudoPrefix(), command, ...args] : [command, ...args];
    const p = spawn(full[0], full.slice(1), { cwd: o.cwd, env: { ...process.env, ...(o.env || {}), DEBIAN_FRONTEND: 'noninteractive' }, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '', i = 0; const t = tty ? setInterval(() => process.stdout.write(`\r  ${ember(frames[i++ % frames.length])} ${label}…   `), 90) : null;
    if (!tty) console.log(`  … ${label}`);
    p.stdout.on('data', (d) => { out += d; }); p.stderr.on('data', (d) => { out += d; });
    p.on('error', (e) => { out += String(e.message); });
    p.on('close', (code) => { if (t) { clearInterval(t); process.stdout.write('\r' + ' '.repeat(Math.min(W, label.length + 12)) + '\r'); } resolve({ ok: code === 0, out: out.trim() }); });
  });
}
async function must(label, command, args, o) {
  const r = await run(label, command, args, o);
  if (r.ok) ok(label); else { bad(label + ' did not work.'); console.log(dim(wrap(r.out.split('\n').slice(-6).join('\n'), '     ', W - 8))); throw new Error(label + ' failed'); }
  return r;
}
const sudoWrite = async (file, text, mode = '644') => {
  if (DRY) { console.log('  ' + dim(`[dry run] write ${file}`)); return; }
  if (PREFIX || isRoot) { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, text, { mode: parseInt(mode, 8) }); return; }
  const tmp = path.join(os.tmpdir(), 'hearth-' + process.pid + '-' + Math.random().toString(36).slice(2)); fs.writeFileSync(tmp, text);
  const r = await run('write ' + file, 'install', ['-D', '-m', mode, tmp, file], { sudo: true }); fs.rmSync(tmp, { force: true });
  if (!r.ok) throw new Error('could not write ' + file + ': ' + r.out);
};
const sysPath = (p) => (PREFIX ? path.join(PREFIX, p) : p);

// ───────────────────────── facts about this computer ─────────────────────────
function osInfo() {
  let rel = {}; try { fs.readFileSync('/etc/os-release', 'utf8').split('\n').forEach((l) => { const m = /^(\w+)=(.*)$/.exec(l); if (m) rel[m[1]] = m[2].replace(/"/g, ''); }); } catch { /* not linux */ }
  const pm = have('apt-get') ? 'apt' : have('dnf') ? 'dnf' : have('pacman') ? 'pacman' : null;
  return { name: rel.PRETTY_NAME || os.type(), id: rel.ID || '', pm };
}
function fetchText(url, ms = 6000) {
  return new Promise((resolve) => {
    const u = new URL(url), m = u.protocol === 'https:' ? https : http;
    const r = m.get(url, { timeout: ms }, (rs) => { let b = ''; rs.on('data', (d) => { b += d; }); rs.on('end', () => resolve(b.trim())); });
    r.on('error', () => resolve('')); r.on('timeout', () => { r.destroy(); resolve(''); });
  });
}
async function publicIp() { for (const u of ['https://api.ipify.org', 'https://ifconfig.me/ip', 'https://icanhazip.com']) { const ip = await fetchText(u); if (/^\d{1,3}(\.\d{1,3}){3}$/.test(ip)) return ip; } return ''; }
const localJson = (port, method, p, token, body) => new Promise((resolve, reject) => {
  const data = body ? Buffer.from(JSON.stringify(body)) : null;
  const r = http.request({ host: '127.0.0.1', port, path: p, method, timeout: 8000, headers: { Authorization: 'Bearer ' + token, ...(data ? { 'Content-Type': 'application/json', 'Content-Length': data.length } : {}) } }, (rs) => {
    let b = ''; rs.on('data', (d) => { b += d; }); rs.on('end', () => { try { resolve(JSON.parse(b || '{}')); } catch { reject(new Error('bad answer')); } });
  });
  r.on('error', reject); r.on('timeout', () => { r.destroy(); reject(new Error('timed out')); }); r.end(data || undefined);
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ───────────────────────── files we write ─────────────────────────
const unitText = ({ user, home, node, url, name, relayDir, cfgDir, projects, host }) => `[Unit]
Description=Hearth server (Claude Code chats for your phone and computers)
After=network-online.target
Wants=network-online.target

[Service]
User=${user}
WorkingDirectory=${relayDir}
ExecStart=${node} ${relayDir}/standalone.js --host ${host} --port ${PORT} --projects ${projects} --dir ${cfgDir} --url ${url} --name ${JSON.stringify(name)}
Environment=HOME=${home}
Environment=PATH=/usr/local/bin:/usr/bin:/bin:${home}/.local/bin
Restart=always
RestartSec=3
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=full
ProtectKernelTunables=yes
ProtectControlGroups=yes

[Install]
WantedBy=multi-user.target
`;
const caddyText = (domain, upstream, dir) => `# Written by the Hearth installer
${domain} {
	header {
		Strict-Transport-Security "max-age=31536000"
		X-Content-Type-Options "nosniff"
		-Server
	}
	import ${dir}/*.caddy
	reverse_proxy ${upstream} {
		flush_interval -1
	}
}
`;
const dockerfileText = () => `FROM node:22-slim
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates && rm -rf /var/lib/apt/lists/* \\
 && npm install -g @anthropic-ai/claude-code
WORKDIR /app
COPY --chown=node:node relay/ /app/relay/
USER node
ENV HOME=/home/node
EXPOSE ${PORT}
CMD ["node", "/app/relay/standalone.js", "--host", "0.0.0.0", "--port", "${PORT}", "--projects", "/data/projects", "--dir", "/data/config"]
`;
const composeText = ({ url, name, withCaddy }) => `# Written by the Hearth installer. Start: docker compose up -d
services:
  hearth:
    build: .
    restart: unless-stopped
    command: ["node", "/app/relay/standalone.js", "--host", "0.0.0.0", "--port", "${PORT}", "--projects", "/data/projects", "--dir", "/data/config", "--url", "${url}", "--name", ${JSON.stringify(name)}]
    volumes:
      - ./data/projects:/data/projects
      - ./data/config:/data/config
      - ./data/claude:/home/node/.claude
      - ./data/claude.json:/home/node/.claude.json
${withCaddy ? '' : `    ports:\n      - "${PORT}:${PORT}"\n`}${withCaddy ? `  caddy:
    image: caddy:2
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - ./data/caddy:/data
    depends_on:
      - hearth
` : ''}`;

// ───────────────────────── the install ─────────────────────────
async function install() {
  banner();
  say(bold('Hello!') + ' I will set up your Hearth server. It takes about five minutes.');
  gap();
  say('A ' + bold('server') + ' is a computer that is always on, far away from you, like a robot that never sleeps. Hearth lives there, so your phone can talk to Claude any time, even when your laptop is off.');
  gap(); say('You only need to answer a few easy questions. If you are not sure, just press ' + bold('Enter') + ': I pick the easy choice for you.');
  gap(); await pause();

  // 0 ── is Hearth already here? Never overwrite someone else's
  const who = isRoot && !PREFIX ? 'hearth' : (process.env.SUDO_USER || os.userInfo().username);
  const here = findInstances(), mine = here.find((i) => i.meta.user === who);
  if (opt('instance')) setInstance(opt('instance'));
  else if (mine) setInstance(mine.instance);
  else if (here.length) { setInstance('hearth-' + who.toLowerCase().replace(/[^a-z0-9]/g, '').slice(0, 20)); }
  const existing = here.find((i) => i.instance === INSTANCE);
  if (here.length && !mine && !opt('instance')) {
    gap(); warn('Hearth is already installed on this server for ' + bold(here.map((i) => i.meta.user + ' (' + i.meta.url + ')').join(', ')) + '.');
    say('I will not touch it. I set up a separate copy for ' + bold(who) + ' next to it, with its own address, its own service and its own key.');
    gap();
  }
  if (existing && !mine && opt('instance')) { bad('That Hearth belongs to ' + existing.meta.user + ', not to you. Choose another --instance name.'); process.exit(1); }
  PORT = existing ? existing.meta.port : await (async () => { const used = new Set(here.map((i) => i.meta.port)); for (let p = 47601; p < 47700; p++) if (!used.has(p) && (PREFIX || await portFree(p))) return p; return 47601; })();

  // 1 ── look around
  step(1, 6, 'Looking around');
  const info = osInfo(), TOTAL_OK = [];
  ok(`This computer runs ${info.name}`);
  if (!PREFIX && process.platform !== 'linux') { bad('This installer is for Linux servers. On a Mac or Windows computer use the Hearth app itself.'); process.exit(1); }
  if (Number(process.versions.node.split('.')[0]) < 18) { bad('Node.js 18 or newer is needed. You have ' + process.version + '.'); process.exit(1); }
  ok('Node.js ' + process.version);
  const dockerOk = have('docker') && spawnSync('docker', ['compose', 'version']).status === 0;
  if (dockerOk) ok('Docker is installed'); else say(dim('Docker is not installed (that is fine).'));
  if (!isRoot && !PREFIX) {
    if (have('sudo')) { say('I need your computer password once, so I can install things. Nothing is shown while you type it.'); if (!DRY && !YES) spawnSync('sudo', ['-v'], { stdio: 'inherit' }); if (!DRY && spawnSync('sudo', ['-n', 'true']).status !== 0) { bad('I could not get administrator rights. Try again, or run this as the root user.'); process.exit(1); } ok('I have the rights I need'); }
    else { bad('This needs administrator rights, but "sudo" is missing. Log in as root and run it again.'); process.exit(1); }
  }
  const ip = DRY && !PREFIX ? '203.0.113.7' : await publicIp();
  if (ip) ok('Your server can be found on the internet at ' + ip); else warn('I could not find your internet address. That is OK if you have your own domain name.');
  const claudeAlready = have('claude');
  gap();

  // 2 ── how
  step(2, 6, 'How should I install it?');
  const mode = opt('mode') ? (opt('mode') === 'docker' ? 1 : 0) : await choose('Where should Hearth live?', [
    { label: 'Directly on this server', hint: 'Simple and light. Hearth runs as a normal program that starts by itself.' },
    { label: 'Inside Docker', hint: dockerOk ? 'Neat and tidy: everything stays inside one box that is easy to remove.' : 'Docker is not installed on this server, so this will not work yet.' },
  ], dockerOk ? 0 : 0);
  const docker = mode === 1;
  if (docker && !dockerOk && !PREFIX) { bad('Docker is not installed. Install it (see docs.docker.com/engine/install), or choose the first option.'); process.exit(1); }
  gap();

  // 3 ── address
  step(3, 6, 'Choose an address');
  say('Your phone and laptop need an address to find the server. It has to be ' + bold('safe') + ' (a padlock, called HTTPS) so nobody can listen in.');
  gap();
  const defaultFree = ip ? ip.replace(/\./g, '-') + '.sslip.io' : '';
  let addrChoice = opt('address');
  if (!addrChoice) {
    const i = await choose('Which address do you want?', [
      { label: 'Get a free address for me' + (defaultFree ? ` (${defaultFree})` : ''), hint: 'Works right away. No account or shopping needed. The server must be reachable from the internet.' },
      { label: 'I have my own domain name', hint: 'For example hearth.mywebsite.com. It must already point to this server.' },
      { label: 'No padlock, only my home network', hint: 'Only for testing. Others on the network could listen in. Not for the open internet.' },
    ], ip ? 0 : 1);
    addrChoice = i === 0 ? 'auto' : i === 1 ? 'domain' : 'none';
  }
  let domain = '';
  if (addrChoice === 'auto') { if (!defaultFree) { bad('I could not find your internet address, so I cannot make a free one. Use your own domain instead.'); process.exit(1); } domain = defaultFree; }
  else if (addrChoice === 'none') domain = '';
  else { domain = addrChoice !== 'domain' ? addrChoice : (await ask('Type your domain name', '')).replace(/^https?:\/\//, '').replace(/\/.*$/, ''); if (!/^[a-z0-9.-]+\.[a-z]{2,}$/i.test(domain)) { bad('That does not look like a domain name.'); process.exit(1); } }
  { const clash = findInstances().find((i) => i.instance !== INSTANCE && domain && i.meta.domain === domain); if (clash) { bad('The address ' + domain + ' already belongs to the Hearth of ' + clash.meta.user + '. Choose another address.'); process.exit(1); } }
  const url = domain ? 'https://' + domain : `http://${ip || os.hostname()}:${PORT}`;
  ok('Address: ' + bold(url));
  if (domain) say(dim('Ports 80 and 443 must be open to the internet (many hosting companies call this the "firewall"). I will open them on this server too.'));
  const name = opt('name') || await ask('What should your server be called?', 'My server');
  gap();

  // 4 ── install
  step(4, 6, 'Installing');
  const user = who;
  let home = isRoot && !PREFIX ? '/home/hearth' : os.homedir();
  if (PREFIX) home = path.join(PREFIX, 'home', user);
  const relayDir = path.join(INSTALL_DIR, 'relay'), cfgDir = docker ? '/data/config' : path.join(home, '.config', 'hearth'), projects = path.join(home, 'projects');
  const meta = { instance: INSTANCE, bin: binOf(INSTANCE), mode: docker ? 'docker' : 'direct', url, domain, name, user, home, projects, cfgDir: docker ? path.join(INSTALL_DIR, 'data', 'config') : cfgDir, version: VERSION, port: PORT };
  try {
    if (!docker) {
      if (isRoot && !PREFIX && spawnSync('id', ['hearth']).status !== 0) await must('Create a user called "hearth"', 'useradd', ['-m', '-s', '/bin/bash', 'hearth']);
      await must('Copy Hearth to ' + INSTALL_DIR, 'sh', ['-c', `mkdir -p '${relayDir}' && cp ${RELAY_FILES.map((f) => `'${RELAY_SRC}/${f}'`).join(' ')} '${relayDir}/' && mkdir -p '${INSTALL_DIR}/installer' && cp '${__dirname}/hearth-server.js' '${INSTALL_DIR}/installer/' && chmod -R a+rX '${INSTALL_DIR}' && chown -R ${user} '${relayDir}' '${INSTALL_DIR}/installer' 2>/dev/null || true`], { sudo: !PREFIX });
      await must('Make the projects folder', 'sh', ['-c', `mkdir -p '${projects}' '${cfgDir}' && chown -R ${user} '${projects}' '${cfgDir}' 2>/dev/null || true`], { sudo: !PREFIX });
      if (!claudeAlready && !PREFIX) await must('Install Claude Code (this is the part that does the thinking)', 'npm', ['install', '-g', '@anthropic-ai/claude-code'], { sudo: true });
      else ok('Claude Code is already installed');
      await sudoWrite(sysPath('/etc/systemd/system/' + INSTANCE + '.service'), unitText({ user, home, node: nodePath(), url, name, relayDir, cfgDir, projects, host: domain ? '127.0.0.1' : '0.0.0.0' }));
      if (!PREFIX) await must('Start Hearth, now and every time the server starts', 'sh', ['-c', `systemctl daemon-reload && systemctl enable --now ${INSTANCE} && systemctl restart ${INSTANCE}`], { sudo: true });
      if (domain) await setupCaddy(info, domain, '127.0.0.1:' + PORT);
    } else {
      await must('Copy Hearth to ' + INSTALL_DIR, 'sh', ['-c', `mkdir -p '${INSTALL_DIR}/relay' '${INSTALL_DIR}/data/projects' '${INSTALL_DIR}/data/config' '${INSTALL_DIR}/data/claude' '${INSTALL_DIR}/data/caddy' '${INSTALL_DIR}/installer' && cp ${RELAY_FILES.map((f) => `'${RELAY_SRC}/${f}'`).join(' ')} '${INSTALL_DIR}/relay/' && cp '${__dirname}/hearth-server.js' '${INSTALL_DIR}/installer/' && { [ -s '${INSTALL_DIR}/data/claude.json' ] || echo '{}' > '${INSTALL_DIR}/data/claude.json'; } && chown -R 1000:1000 '${INSTALL_DIR}/data'`], { sudo: !PREFIX });
      await sudoWrite(path.join(INSTALL_DIR, 'Dockerfile'), dockerfileText());
      await sudoWrite(path.join(INSTALL_DIR, 'docker-compose.yml'), composeText({ url, name, withCaddy: !!domain }));
      if (domain) await sudoWrite(path.join(INSTALL_DIR, 'Caddyfile'), caddyText(domain, 'hearth:' + PORT, '/nonexistent').replace(/\timport .*\n/, ''));
      if (domain && !PREFIX) await openFirewall();
      if (!PREFIX) await must('Build and start Hearth (the first time takes a few minutes)', 'docker', ['compose', 'up', '-d', '--build'], { sudo: true, cwd: INSTALL_DIR });
    }
    await sudoWrite(path.join(INSTALL_DIR, 'install.json'), JSON.stringify(meta, null, 2));
    if (!PREFIX) await sudoWrite('/usr/local/bin/' + binOf(INSTANCE), `#!/bin/sh\nexec node ${INSTALL_DIR}/installer/hearth-server.js ${INSTANCE === 'hearth' ? '' : '--instance ' + INSTANCE} "$@"\n`, '755');
  } catch (e) { gap(); bad('Something went wrong: ' + e.message); say('Nothing is broken that cannot be fixed. Read the message above, fix that one thing, and run the installer again. It is safe to run it twice.'); process.exit(1); }

  // wait until it answers
  if (!DRY && !PREFIX) {
    let up = false; for (let i = 0; i < 40 && !up; i++) { up = await tcpUp(); if (!up) await sleep(1500); }
    if (!up) { bad('Hearth did not start. Look at what it says with:  ' + (docker ? 'cd ' + INSTALL_DIR + ' && sudo docker compose logs' : 'sudo journalctl -u hearth -n 30')); process.exit(1); }
    ok('Hearth is running');
  }

  // 5 ── Claude sign in
  step(5, 6, 'Sign in to Claude');
  say('Claude Code needs to know who you are, so it can use your Claude account. You only do this once.');
  if (!DRY && !PREFIX && !(await signedIn(meta)) && !YES) {
    say('I will open Claude now. It shows a web link. Open the link on any device, sign in, and paste the code it gives you back here. When Claude is ready for a message, type  ' + bold('/exit') + '  and press Enter.');
    gap(); await pause('Press Enter to open Claude');
    const args = docker ? ['compose', 'run', '--rm', '-it', 'hearth', 'claude'] : (isRoot ? ['su', '-', user, '-c', 'claude'] : ['sudo', '-u', user, '-H', 'claude']);
    spawnSync(docker ? 'sudo' : args[0], docker ? ['docker', ...args] : args.slice(1), { stdio: 'inherit', cwd: docker ? INSTALL_DIR : undefined });
    if (await signedIn(meta)) ok('You are signed in'); else warn('I could not see a sign-in yet. You can do it later with:  ' + (docker ? `cd ${INSTALL_DIR} && sudo docker compose run --rm -it hearth claude` : `sudo -u ${user} -H claude`));
  } else if (!DRY && !PREFIX && YES && !(await signedIn(meta))) warn('Not signed in yet. Run  hearth-server login  later (or: sudo -u ' + user + ' -H claude).');
  else ok(DRY || PREFIX ? 'Sign-in skipped (test run)' : 'You are already signed in');

  // 6 ── connect devices
  step(6, 6, 'Connect your laptop and phone');
  await showCode(meta, true);
}

async function setupCaddy(info, domain, upstream) {
  if (!have('caddy') && !PREFIX) {
    if (info.pm === 'apt') await must('Install Caddy (it makes the safe padlock)', 'apt-get', ['install', '-y', 'caddy'], { sudo: true });
    else if (info.pm === 'dnf') await must('Install Caddy (it makes the safe padlock)', 'dnf', ['install', '-y', 'caddy'], { sudo: true });
    else throw new Error('I do not know how to install Caddy on this system. Install it from caddyserver.com, or choose the Docker option');
  } else ok('Caddy is already installed');
  await sudoWrite(caddyDirOf(INSTANCE) + '/00-hearth.caddy', '# extra routes for this Hearth (the support desk adds one here)\n');
  await sudoWrite(caddyFileOf(INSTANCE), caddyText(domain, upstream, '/etc/caddy/' + INSTANCE + '.d'));
  const main = sysPath('/etc/caddy/Caddyfile');
  if (!PREFIX) {
    const has = fs.existsSync(main) ? fs.readFileSync(main, 'utf8') : '';
    const line = 'import /etc/caddy/' + INSTANCE + '.caddy';
    if (!has.split('\n').some((l) => l.trim() === line)) await sudoWrite(main, (has ? has.replace(/\s*$/, '\n\n') : '') + line + '\n');
  }
  if (!PREFIX && !DRY) {   // Caddy sometimes stops by itself (older versions); make the system start it again within seconds
    await sudoWrite('/etc/systemd/system/caddy.service.d/hearth.conf', '[Service]\nRestart=always\nRestartSec=3\n');
    const cv = (spawnSync('caddy', ['version'], { encoding: 'utf8' }).stdout || '').match(/v?(\d+)\.(\d+)/);
    if (cv && (Number(cv[1]) < 2 || (Number(cv[1]) === 2 && Number(cv[2]) < 7))) warn('Your Caddy is old (' + cv[0] + '). It can crash now and then; Hearth restarts it automatically. Updating Caddy (caddyserver.com/docs/install) is recommended.');
  }
  await openFirewall();
  if (!PREFIX) await must('Turn on the padlock', 'sh', ['-c', 'systemctl enable caddy && systemctl restart caddy'], { sudo: true });
}
async function openFirewall() {
  if (PREFIX || DRY) return;
  if (have('ufw') && /Status: active/.test(spawnSync('sh', ['-c', `${sudoPrefix().join(' ')} ufw status`], { encoding: 'utf8' }).stdout || '')) await must('Open the doors for the internet (ports 80 and 443)', 'sh', ['-c', 'ufw allow 80/tcp && ufw allow 443/tcp'], { sudo: true });
  else if (have('firewall-cmd')) await run('Open the doors for the internet (ports 80 and 443)', 'sh', ['-c', 'firewall-cmd --permanent --add-service=http --add-service=https && firewall-cmd --reload'], { sudo: true });
}
const tcpUp = () => new Promise((res) => { const r = http.get({ host: '127.0.0.1', port: PORT, path: '/info', timeout: 2000 }, (rs) => { rs.resume(); res(true); }); r.on('error', () => res(false)); r.on('timeout', () => { r.destroy(); res(false); }); });
async function signedIn(meta) {
  const home = meta.mode === 'docker' ? path.join(INSTALL_DIR, 'data', 'claude') : path.join(meta.home, '.claude');
  const f = path.join(home, '.credentials.json');
  const r = spawnSync('sh', ['-c', `${sudoPrefix().join(' ')} test -s '${f}'`]); return r.status === 0;
}

/** The finish line: one address and one short code. */
async function showCode(meta, first) {
  let token = '';
  const cfg = path.join(meta.cfgDir, 'relay.json');
  try { token = JSON.parse(spawnSync('sh', ['-c', `${sudoPrefix().join(' ')} cat '${cfg}'`], { encoding: 'utf8' }).stdout).token; } catch { /* below */ }
  let code = '';
  if (!DRY && !PREFIX && token) {
    try {
      if (meta.mode === 'docker') {
        const js = `fetch('http://127.0.0.1:${PORT}/pair/new',{method:'POST',headers:{Authorization:'Bearer ${token}','Content-Type':'application/json'},body:'{}'}).then(r=>r.json()).then(j=>console.log(j.code))`;
        code = spawnSync('sudo', ['docker', 'compose', 'exec', '-T', 'hearth', 'node', '-e', js], { cwd: INSTALL_DIR, encoding: 'utf8' }).stdout.trim();
      } else code = (await localJson(PORT, 'POST', '/pair/new', token, {})).code;
    } catch { /* below */ }
  }
  if (DRY || PREFIX) code = '123456';
  if (!code) { bad('I could not make a code. Is Hearth running?  Try:  ' + binOf(INSTANCE) + ' status'); return; }
  const pretty = code.replace(/(\d{3})(\d{3})/, '$1 $2');
  const link = `hearth://pair?${new URLSearchParams({ url: meta.url, token: token || '', name: meta.name })}`;
  gap();
  console.log(ember('  ┌' + '─'.repeat(54) + '┐'));
  console.log(ember('  │') + bold('   Your server is ready! ') + ' '.repeat(29) + ember('│'));
  console.log(ember('  │') + ' '.repeat(54) + ember('│'));
  console.log(ember('  │') + '   Address:  ' + bold(meta.url.padEnd(41)) + ember('│'));
  console.log(ember('  │') + '   Code:     ' + bold(ember(pretty.padEnd(41))) + ember('│'));
  console.log(ember('  └' + '─'.repeat(54) + '┘'));
  gap();
  say(bold('Now connect your devices.') + ' Do this on each one:');
  gap();
  item(1, 'Install the Hearth app (from github.com/veelvoer/hearth/releases).');
  item(2, 'Open it. It asks for a server. Type the ' + bold('address') + ' and the ' + bold('code') + ' from the box above.');
  item(3, 'That is all. Your laptop and phone now share the same chats and project files.');
  gap();
  say(dim('The code works for 10 minutes. Type  ' + binOf(INSTANCE) + ' code  on this server any time to see the address, a new code and the link again, for example to connect another computer.'));
  say(dim('Or paste this one link on a device instead (keep it secret, it opens your server):'));
  console.log('  ' + dim(link));
  gap();
  if (first) say('Have fun! 🔥');
}

async function main() {
  if (cmd !== 'install') {   // which Hearth on this server? the one named by --instance, else the one of this user, else the only one
    const all = findInstances(), me = process.env.SUDO_USER || os.userInfo().username;
    const pick = opt('instance') ? all.find((i) => i.instance === opt('instance')) : all.find((i) => i.meta.user === me) || (all.length === 1 ? all[0] : null);
    if (pick) { setInstance(pick.instance); PORT = pick.meta.port || PORT; }
  }
  const meta = (() => { try { return JSON.parse(fs.readFileSync(path.join(INSTALL_DIR, 'install.json'), 'utf8')); } catch { return null; } })();
  if (cmd === 'install') return install();
  if (!meta) { bad('Hearth is not installed here yet. Run the installer first.'); process.exit(1); }
  if (cmd === 'code' || cmd === 'link') { banner(); return showCode(meta, false); }
  if (cmd === 'status') {
    banner(); const up = await tcpUp(); (up ? ok : bad)(up ? 'Hearth is running at ' + meta.url : 'Hearth is not answering.');
    say(dim(meta.mode === 'docker' ? `Docker folder: ${INSTALL_DIR}` : `Logs: sudo journalctl -u ${INSTANCE} -n 30`)); return undefined;
  }
  if (cmd === 'update') {
    banner(); say('To update, run the installer again from the newest version:'); gap(); say('curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/install.sh | bash'); return undefined;
  }
  if (cmd === 'uninstall') {
    banner(); say('This removes the Hearth server program. Your projects and chats stay where they are.'); if (!(await confirm('Remove Hearth from this server?', false))) return undefined;
    if (meta.mode === 'docker') await run('Stop Hearth', 'docker', ['compose', 'down'], { sudo: true, cwd: INSTALL_DIR });
    else await run('Stop Hearth', 'sh', ['-c', `systemctl disable --now ${INSTANCE}; rm -f /etc/systemd/system/${INSTANCE}.service /etc/caddy/${INSTANCE}.caddy; sed -i "\\|^import /etc/caddy/${INSTANCE}.caddy$|d" /etc/caddy/Caddyfile; systemctl daemon-reload; systemctl restart caddy 2>/dev/null; true`], { sudo: true });
    await run('Remove the program files', 'sh', ['-c', `rm -rf '${INSTALL_DIR}/relay' '${INSTALL_DIR}/installer' '${INSTALL_DIR}/install.json' /usr/local/bin/${binOf(INSTANCE)} /etc/caddy/${INSTANCE}.d`], { sudo: true });
    ok('Hearth is removed.'); return undefined;
  }
  say('Commands:  code (address, code and link for a new device) · status · update · uninstall'); return undefined;
}
main().then(() => process.exit(0), (e) => { bad(e.message); process.exit(1); });
