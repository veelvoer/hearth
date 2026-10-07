'use strict';
/* First-run tutorial: what the app is, check Claude Code, optional server, connect a phone. Every step can be skipped. */
const OB = { el: null, step: 0, server: false, phoneMode: null };

function startTutorial() {
  if (OB.el) return;
  OB.step = 0; OB.el = h('div', { class: 'ob' }); document.body.append(OB.el); drawTutorial();
}
async function endTutorial() {
  if (OB.el) { OB.el.remove(); OB.el = null; }
  try { S.settings = await cm.setSettings({ onboarded: true }); } catch { /* ignore */ }
  if (typeof render === 'function') render();
}
const go = (n) => { OB.step = n; drawTutorial(); };

const STEPS = ['Welcome', 'Your server', 'Claude Code', 'Your phone', 'Your projects', 'Ready'];
const INSTALL_CMD = 'curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/install.sh | bash';

function drawTutorial() {
  const card = h('div', { class: 'obcard' });
  const dots = h('div', { class: 'obdots' }, STEPS.map((l, i) => h('span', { class: 'obdot' + (i === OB.step ? ' on' : i < OB.step ? ' done' : ''), title: l })));
  const body = h('div', { class: 'obbody' });
  const foot = h('div', { class: 'obfoot' });
  const last = STEPS.length - 1;
  card.append(dots, body, foot);
  OB.el.innerHTML = ''; OB.el.append(card);
  const next = (label, fn) => h('button', { class: 'primary', onclick: fn || (() => go(OB.step + 1)) }, label || 'Continue');
  const back = OB.step > 0 && OB.step < last ? h('button', { class: 'ghost', onclick: () => go(OB.step - 1) }, 'Back') : null;
  const skip = OB.step < last ? h('button', { class: 'link', onclick: endTutorial }, 'Skip tutorial') : null;
  const title = (t, sub) => body.append(h('h2', {}, t), sub ? h('p', { class: 'muted' }, sub) : null);
  const list = (items, ordered) => h(ordered ? 'ol' : 'ul', { class: 'oblist' }, items.map((t) => h('li', {}, t)));

  if (OB.step === 0) {
    body.append(h('div', { class: 'oblogo', html: sparkSVG(84) }));
    title('Welcome to Hearth', 'Talk to Claude Code from your computer and your phone. Your chats and projects stay together on every device.');
    body.append(list(['Start and continue chats from anywhere', 'Make a project on your phone, and find it on your laptop', 'Get a message (or a call) when Claude is done or needs you', 'See how much of your Claude limit is left']),
      h('p', { class: 'small muted' }, 'This takes about five minutes. You can skip any step and come back from Settings.'));
    foot.append(skip, h('div', { class: 'grow' }), next('Get started'));
  } else if (OB.step === 1) {
    title('First, set up your server', 'A server is a computer that is always on, far away, like a robot that never sleeps. Hearth lives there, so your phone works even when this laptop is closed.');
    body.append(h('div', { class: 'obnote' }, h('b', {}, 'Already have a server?'), list(['Open a terminal on the server (for example with ssh).', 'Copy this line, paste it there and press Enter:'], true),
      h('div', { class: 'obcmd' }, h('code', {}, INSTALL_CMD), h('button', { class: 'ghost sm', onclick: (e) => { cm.copy ? cm.copy(INSTALL_CMD) : navigator.clipboard.writeText(INSTALL_CMD); e.target.textContent = 'Copied ✓'; setTimeout(() => { e.target.textContent = 'Copy'; }, 1500); } }, 'Copy')),
      h('div', { class: 'small muted' }, 'It asks a few easy questions and then shows an address and a 6-digit code. Type them below.')));
    const addr = h('input', { placeholder: 'Address, like https://1-2-3-4.sslip.io' }), code = h('input', { placeholder: '6-digit code', inputmode: 'numeric', style: 'font-family:ui-monospace,monospace;letter-spacing:.12em' }), msg = h('div', { class: 'small' });
    const connect = async () => { msg.className = 'small'; msg.textContent = 'Connecting…'; try { await cm.addMachine({ name: '', host: addr.value, token: code.value }); msg.className = 'small okc'; msg.textContent = '✓ Connected to your server'; OB.connected = true; } catch (e) { msg.className = 'small err'; msg.textContent = clean(e); } };
    body.append(addr, code, h('button', { class: 'primary sm', onclick: connect }, 'Connect'), msg,
      h('details', { class: 'small muted' }, h('summary', {}, 'I don’t have a server'), h('p', {}, 'Any small Linux server works (about 4 to 6 euros a month at most hosting companies). Pick the cheapest one with Debian or Ubuntu, then come back here. You can also skip this step: Hearth then works only while this computer is on.')));
    foot.append(back, skip, h('div', { class: 'grow' }), next(OB.connected ? 'Continue' : 'Later'));
  } else if (OB.step === 2) {
    title('Is Claude Code installed here?', 'Hearth uses Claude Code, the tool made by Anthropic. It must be on every computer where you work on projects.');
    const res = h('div', { class: 'obnote' }, 'Checking…'); body.append(res);
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
    title('Add your phone', 'Optional. You can do this later from Settings.');
    body.append(list(['Install the Hearth app on your phone.', 'Open it. It asks for your server.', 'Type the address and the code shown below.'], true));
    const box = h('div', { class: 'obnote' }, 'Getting a code…'); body.append(box);
    (async () => {
      try {
        let st = null; try { st = await cm.hubStatus(); } catch { /* none */ }
        let code, addrs;
        if (st && st.server) { const r = await cm.phoneCode(); code = r.code; addrs = [r.address]; }
        else { const i = await cm.pairInfo(); code = i.code; addrs = (i.addresses || []).map((a) => a + (i.port === 47601 ? '' : ':' + i.port)); }
        box.className = 'obnote'; box.innerHTML = '';
        box.append(h('div', { class: 'obcode' }, String(code || '').replace(/(\d{3})(\d{3})/, '$1 $2')), h('div', { class: 'small muted' }, 'Address: ' + (addrs.filter(Boolean).join(' or ') || 'not available') + (st && st.server ? '' : '  ·  phone and computer on the same Wi-Fi')), h('div', { class: 'small muted' }, 'The code works for 10 minutes. You can get a new one in Settings.'));
      } catch (e) { box.className = 'obnote bad'; box.textContent = 'Couldn’t make a code right now: ' + clean(e); }
    })();
    foot.append(back, h('button', { class: 'link', onclick: () => go(4) }, 'Do this later'), h('div', { class: 'grow' }), next('Done'));
  } else if (OB.step === 4) {
    title('How your projects travel', 'Your projects live in one folder called “projects” in your home folder. Hearth keeps that folder the same on this computer and on your server.');
    body.append(list(['Make a file or project here: it appears on the server and your phone.', 'Start a project on your phone: it is made on the server.', 'If this computer was off, Hearth tells you next time: “A project was made on your server. Install it?” Press Install to copy it here, or Later and it asks again when you open Hearth.', 'Deleted files go to a safety folder (.hearth-trash) first, and Hearth refuses to delete a lot at once. Your work is safe.']),
      h('p', { class: 'small muted' }, 'Your chats travel the same way, so a chat you start on your phone continues here.'));
    foot.append(back, skip, h('div', { class: 'grow' }), next());
  } else {
    body.append(h('div', { class: 'oblogo', html: sparkSVG(84) }));
    title('You’re all set', 'A few things worth knowing:');
    body.append(list(['Press “New chat”, pick a project folder and say what you want.', 'Type / in a message to see commands, skills and shortcuts.', 'Add-ons in a chat lets you add skills, plugins and connections.', 'You can replay this tutorial any time from Settings → Help.']),
      h('p', { class: 'small muted' }, 'Hearth is an independent app and is not made by or affiliated with Anthropic. “Claude” is a trademark of Anthropic.'));
    foot.append(h('div', { class: 'grow' }), next('Open Hearth', endTutorial));
  }
}
