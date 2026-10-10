'use strict';
/* First-run setup: one friendly path that sets everything up with you (server, Claude Code, projects folder, voice, look, phone).
   Every step can be skipped and the whole thing can be replayed from Settings. Afterwards the guided tour explains the app. */
const OB = { el: null, step: 0, where: 'server', connected: false, dir: null, moved: [], voiceLog: [], voiceBusy: false, voiceDone: false };
const INSTALL_CMD = 'curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/install.sh | bash';
const STEPS = ['Welcome', 'Where', 'Claude Code', 'Projects', 'Voice', 'Look', 'Phone', 'Ready'];

function startTutorial() {
  if (OB.el) return;
  OB.step = 0; OB.el = h('div', { class: 'ob' }); document.body.append(OB.el); drawTutorial();
  cm.setSettings({ onboarded: true }).then((s) => { if (s) S.settings = s; }).catch(() => {});   // shows once, even if the app is closed halfway
}
async function endTutorial(thenTour) {
  if (OB.el) { OB.el.classList.add('leaving'); const el = OB.el; OB.el = null; setTimeout(() => el.remove(), 260); }
  try { S.settings = await cm.setSettings({ onboarded: true, setupDone: true }); } catch { /* ignore */ }
  if (typeof render === 'function') render();
  if (thenTour && typeof startTour === 'function') setTimeout(startTour, 500);
}
const go = (n) => { OB.dir = n > OB.step ? 'fwd' : 'back'; OB.step = n; drawTutorial(); };
cm.on('voice-install', (d) => {
  if (d.line) { OB.voiceLog.push(d.line); OB.voiceLog = OB.voiceLog.slice(-4); }
  if (d.done) { OB.voiceBusy = false; OB.voiceDone = !!d.ok; if (!d.ok) OB.voiceLog.push('That did not finish. You can try again later from Settings.'); }
  if (OB.el && OB.step === 4) drawTutorial();
});

function drawTutorial() {
  if (!OB.el) return;
  const card = h('div', { class: 'obcard' });
  const dots = h('div', { class: 'obdots' }, STEPS.map((l, i) => h('span', { class: 'obdot' + (i === OB.step ? ' on' : i < OB.step ? ' done' : ''), title: l })));
  const body = h('div', { class: 'obbody' + (OB.dir ? ' ' + OB.dir : '') }); OB.dir = null;
  const foot = h('div', { class: 'obfoot' });
  const last = STEPS.length - 1;
  card.append(dots, body, foot);
  const keep = OB.el.querySelector('.obbody'); const scroll = keep && !body.className.includes('fwd') && !body.className.includes('back') ? keep.scrollTop : 0;
  OB.el.innerHTML = ''; OB.el.append(card); if (scroll) body.scrollTop = scroll;
  const next = (label, fn) => h('button', { class: 'primary', onclick: fn || (() => go(OB.step + 1)) }, label || 'Continue');
  const back = OB.step > 0 && OB.step < last ? h('button', { class: 'ghost', onclick: () => go(OB.step - 1) }, 'Back') : null;
  const skip = OB.step < last ? h('button', { class: 'link', onclick: () => endTutorial(false) }, 'Skip setup') : null;
  const put = (...k) => body.append(...k.filter(Boolean));   // null means "nothing here"
  const title = (t, sub) => put(h('h2', {}, t), sub ? h('p', { class: 'muted' }, sub) : null);
  const list = (items, ordered) => h(ordered ? 'ol' : 'ul', { class: 'oblist' }, items.map((t) => h('li', {}, t)));
  const set = async (patch) => { S.settings = await cm.setSettings(patch); await applyPalette(); drawTutorial(); };

  if (OB.step === 0) {
    put(h('div', { class: 'oblogo', html: sparkSVG(84) }));
    title('Welcome to Hearth', 'Let’s set everything up together. It takes about five minutes, and you can skip any step.');
    put(list(['Connect your server (or use this computer)', 'Check that Claude Code is ready', 'Choose where your projects live, so they sync by themselves', 'Pick voices and colors', 'Connect your phone']),
      h('p', { class: 'small muted' }, 'When we are done, a short tour shows you around the app.'));
    foot.append(skip, h('div', { class: 'grow' }), next('Let’s go'));
  } else if (OB.step === 1) {
    title('Where should Hearth run?', 'Pick one. You can change it later in Settings.');
    const opt = (id, name, text) => h('button', { class: 'obopt' + (OB.where === id ? ' on' : ''), onclick: () => { OB.where = id; drawTutorial(); } }, h('b', {}, name), h('span', { class: 'small muted' }, text));
    put(opt('server', 'On a server (recommended)', 'A computer that is always on, far away, like a robot that never sleeps. Your phone works even when this laptop is closed, from anywhere.'),
      opt('computer', 'On this computer', 'No server needed. Your phone connects straight to this computer, so it works while this computer is on and your phone is on the same Wi-Fi.'));
    if (OB.where === 'server') {
      put(h('div', { class: 'obnote' }, h('b', {}, 'Already have a server?'), list(['Open a terminal on the server (for example with ssh).', 'Copy this line, paste it there and press Enter:'], true),
        h('div', { class: 'obcmd' }, h('code', {}, INSTALL_CMD), h('button', { class: 'ghost sm', onclick: (e) => { navigator.clipboard.writeText(INSTALL_CMD); e.target.textContent = 'Copied ✓'; setTimeout(() => { e.target.textContent = 'Copy'; }, 1500); } }, 'Copy')),
        h('div', { class: 'small muted' }, 'It asks a few easy questions and then shows an address and a 6-digit code. Type them below.')));
      const addr = h('input', { placeholder: 'Address, like https://1-2-3-4.sslip.io' }), code = h('input', { placeholder: '6-digit code', inputmode: 'numeric', style: 'font-family:ui-monospace,monospace;letter-spacing:.12em' }), msg = h('div', { class: 'small' });
      const connect = async () => { msg.className = 'small'; msg.textContent = 'Connecting…'; try { await cm.addMachine({ name: '', host: addr.value, token: code.value }); msg.className = 'small okc'; msg.textContent = '✓ Connected to your server'; OB.connected = true; } catch (e) { msg.className = 'small err'; msg.textContent = clean(e); } };
      put(addr, code, h('button', { class: 'primary sm', onclick: connect }, 'Connect'), msg,
        h('details', { class: 'small muted' }, h('summary', {}, 'I don’t have a server'), h('p', {}, 'Any small Linux server works (about 4 to 6 euros a month at most hosting companies). Pick the cheapest one with Debian or Ubuntu, then come back here. Or choose “On this computer” above.')));
    } else {
      put(h('div', { class: 'obnote' }, h('b', {}, 'Good to know'), list(['Chats and projects stay on this computer. Hearth on your phone shows them while both are on the same Wi-Fi.', 'Away from home? Install the free app Tailscale on this computer and your phone. Hearth then works anywhere, still without a server.', 'Advanced: you can also open a port on your router (port forwarding).'])));
    }
    foot.append(back, skip, h('div', { class: 'grow' }), next(OB.where === 'server' && !OB.connected ? 'Later' : 'Continue'));
  } else if (OB.step === 2) {
    title('Is Claude Code installed here?', 'Hearth uses Claude Code, the tool made by Anthropic. It must be on every computer where you work on projects.');
    const res = h('div', { class: 'obnote' }, 'Checking…'); put(res);
    const check = async () => {
      res.className = 'obnote'; res.textContent = 'Checking…';
      let i = null; try { i = await cm.pairInfo(); } catch { /* none */ }
      res.innerHTML = '';
      if (i && i.claude) { res.className = 'obnote good'; res.append('✓ Claude Code is installed on this computer.'); }
      else {
        res.className = 'obnote bad';
        res.append(h('div', {}, 'Claude Code was not found.'), list(['Install it from claude.com/claude-code.', 'Open a terminal once, type claude and sign in with your Claude account.', 'Come back and press Check again.'], true), h('button', { class: 'ghost sm', onclick: check }, 'Check again'));
      }
    };
    check();
    foot.append(back, skip, h('div', { class: 'grow' }), next());
  } else if (OB.step === 3) {
    title('Your projects folder', 'Hearth keeps one folder, called “projects”, the same on this computer, your server and your phone. Nothing to switch on: it syncs by itself.');
    const box = h('div', { class: 'stack' }); put(box);
    (async () => {
      let p = null; try { p = await cm.setupPaths(); } catch { /* shown below */ }
      box.append(h('div', { class: 'obnote good' }, '✓ ' + (p ? p.projects : 'The projects folder') + ' is ready.', h('div', { class: 'small muted' }, 'Every folder you put inside is kept the same everywhere. New projects you start in Hearth are made here.')));
      let cands = []; try { const r = await cm.relay('local', 'GET', '/projects'); const home = (p && p.home) || ''; cands = (r.projects || []).filter((x) => x.path && p && !x.path.startsWith(p.projects + '/') && x.path !== p.projects && x.path !== home && !/^(\/tmp|\/var\/tmp)/.test(x.path) && !OB.moved.includes(x.path)).slice(0, 12); } catch { /* none */ }
      if (!cands.length) { box.append(h('p', { class: 'small muted' }, OB.moved.length ? 'Your projects were moved into the folder.' : 'No other projects found. You can move projects here later: open a chat → Options → Move.')); return; }
      box.append(h('div', { class: 'small muted' }, 'Projects you already have on this computer. Move the ones you want to sync (chats come along):'));
      const checks = cands.map((c) => ({ c, box: h('input', { type: 'checkbox' }) }));
      checks.forEach((x) => box.append(h('label', { class: 'updrow' }, x.box, h('span', {}, h('b', {}, x.c.name), h('span', { class: 'small muted' }, '  ' + x.c.path)))));
      const msg = h('div', { class: 'small' });
      const mv = h('button', { class: 'primary sm', onclick: async () => {
        const pick = checks.filter((x) => x.box.checked).map((x) => x.c); if (!pick.length) { msg.textContent = 'Tick the projects to move, or press Continue to keep them where they are.'; return; }
        mv.disabled = true; msg.className = 'small'; const errs = [];
        for (const c of pick) { msg.textContent = 'Moving ' + c.name + '…'; try { await cm.relay('local', 'POST', '/projects/move', { cwd: c.path, name: c.name.replace(/[^A-Za-z0-9._ -]/g, '-').slice(0, 60) }); OB.moved.push(c.path); } catch (e) { errs.push(c.name + ': ' + clean(e)); } }
        if (errs.length) { msg.className = 'small err'; msg.textContent = errs.join(' · '); mv.disabled = false; } else { drawTutorial(); }
      } }, 'Move the ticked projects');
      box.append(h('div', { class: 'row' }, mv), msg, h('div', { class: 'small muted' }, 'You can always keep a project where it is. It just will not sync.'));
    })();
    foot.append(back, skip, h('div', { class: 'grow' }), next());
  } else if (OB.step === 4) {
    title('Voice', 'Talk to Claude and hear it answer. Pick your language first.');
    const lang = localStorage.getItem('cm.voiceLang') || ((navigator.language || '').startsWith('nl') ? 'nl' : 'en');
    put(h('div', { class: 'row' }, chip('English', lang === 'en', () => { localStorage.setItem('cm.voiceLang', 'en'); drawTutorial(); }), chip('Nederlands', lang === 'nl', () => { localStorage.setItem('cm.voiceLang', 'nl'); drawTutorial(); })));
    const sample = () => { try { const u = new SpeechSynthesisUtterance(lang === 'nl' ? 'Hallo! Zo klink ik als ik antwoord.' : 'Hello! This is how I sound when I answer.'); u.lang = lang === 'nl' ? 'nl-NL' : 'en-US'; speechSynthesis.cancel(); speechSynthesis.speak(u); } catch { /* no voices */ } };
    put(h('div', { class: 'obnote' }, h('b', {}, 'Works right away'), h('div', { class: 'small muted' }, 'Hearth can use the voices that come with your computer. No setup.'), h('button', { class: 'ghost sm', onclick: sample }, '▶ Hear a sample')));
    if (OB.voiceDone) put(h('div', { class: 'obnote good' }, '✓ Better voices are installed and used automatically.'));
    else if (navigator.platform && /linux/i.test(navigator.platform + navigator.userAgent)) {
      put(h('div', { class: 'obnote' }, h('b', {}, 'Better voices (optional)'), h('div', { class: 'small muted' }, 'Natural English and Dutch voices that run on your own computer: nothing is sent anywhere. About 1.2 GB; it takes a few minutes. You can keep using Hearth meanwhile.'),
        OB.voiceBusy ? h('div', { class: 'stack' }, h('div', { class: 'row' }, h('span', { class: 'spin' }), h('span', { class: 'small' }, 'Installing…')), h('pre', { class: 'out small' }, OB.voiceLog.join('\n') || '…'))
          : h('button', { class: 'primary sm', onclick: async () => { OB.voiceBusy = true; OB.voiceLog = []; drawTutorial(); try { await cm.installVoice(); } catch (e) { OB.voiceBusy = false; OB.voiceLog = [clean(e)]; drawTutorial(); } } }, 'Install better voices'),
        !OB.voiceBusy && OB.voiceLog.length ? h('div', { class: 'small err' }, OB.voiceLog.slice(-1)[0]) : null));
    } else put(h('p', { class: 'small muted' }, 'Extra local voices are available on Linux. On this system Hearth uses your computer’s own voices.'));
    foot.append(back, skip, h('div', { class: 'grow' }), next());
  } else if (OB.step === 5) {
    title('Make it yours', 'Pick how Hearth looks. The icon of the app follows your colors.');
    const st = S.settings || {};
    put(h('div', { class: 'small muted' }, 'Mode'), h('div', { class: 'row' }, [['light', 'Light'], ['dark', 'Dark'], ['system', 'Auto']].map(([v, l]) => chip(l, (st.theme || 'system') === v, () => set({ theme: v })))),
      h('div', { class: 'small muted' }, 'Colors'), h('div', { class: 'row wrap' }, [['claude', 'Hearth'], ['system', 'My desktop'], ['custom', 'Pick a color']].map(([v, l]) => chip(l, (st.palette || 'claude') === v, () => set({ palette: v })))),
      st.palette === 'custom' ? h('div', { class: 'row' }, h('input', { type: 'color', value: st.customAccent || '#F0643C', style: 'width:56px;height:38px;padding:2px', onchange: (e) => set({ customAccent: e.target.value }) }), h('span', { class: 'small muted' }, 'Any color you like.')) : null,
      h('div', { class: 'obnote' }, h('div', { class: 'row' }, h('div', { class: 'oblogo', style: 'padding:0', html: sparkSVG(48) }), h('div', {}, h('b', {}, 'Preview'), h('div', { class: 'small muted' }, 'Buttons, highlights and the app icon use this color.'))), h('div', { class: 'row' }, h('button', { class: 'primary sm' }, 'A button'), chip('A chip', true, () => {}))));
    foot.append(back, skip, h('div', { class: 'grow' }), next());
  } else if (OB.step === 6) {
    title('Add your phone', 'Optional. You can do this later from Settings.');
    put(list(['Install the Hearth app on your phone (download it from github.com/veelvoer/hearth/releases).', 'Put the phone on the same Wi-Fi as this computer and open Hearth.', 'Tap “Connect” next to this computer. A card pops up here: press Accept.'], true),
      h('p', { class: 'small muted' }, 'That is all. Your phone gets everything it needs, also the connection to your server if you set one up.'));
    const more = h('details', { class: 'small muted' }, h('summary', {}, 'The phone cannot find this computer?'));
    put(more);
    (async () => {
      try {
        let st = null; try { st = await cm.hubStatus(); } catch { /* none */ }
        let code, addrs;
        if (st && st.server) { const r = await cm.phoneCode(); code = r.code; addrs = [r.address]; }
        else { const i = await cm.pairInfo(); code = i.code; addrs = (i.addresses || []).map((a) => a + (i.port === 47601 ? '' : ':' + i.port)); }
        more.append(h('p', {}, 'Type this on the phone, under “Enter the address myself”:'), h('div', { class: 'obcode' }, String(code || '').replace(/(\d{3})(\d{3})/, '$1 $2')), h('div', {}, 'Address: ' + (addrs.filter(Boolean).join(' or ') || 'not available')), h('div', {}, 'The code works for 10 minutes. You can get a new one in Settings.'));
      } catch (e) { more.append(h('p', {}, 'Could not make a code right now: ' + clean(e))); }
    })();
    foot.append(back, h('button', { class: 'link', onclick: () => go(7) }, 'Do this later'), h('div', { class: 'grow' }), next('Done'));
  } else {
    put(h('div', { class: 'oblogo', html: sparkSVG(84) }));
    title('You’re all set', 'Hearth is ready. Want a quick tour? It points at the important buttons and explains each one (about two minutes).');
    put(h('p', { class: 'small muted' }, 'You can replay the tour or this setup any time from Settings → Help.'),
      h('p', { class: 'small muted' }, 'Hearth is an independent app and is not made by or affiliated with Anthropic. “Claude” is a trademark of Anthropic.'));
    foot.append(h('button', { class: 'ghost', onclick: () => endTutorial(false) }, 'Skip the tour'), h('div', { class: 'grow' }), next('Show me around', () => endTutorial(true)));
  }
}
