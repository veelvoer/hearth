// window.cm for the Rust app: the same calls the pages used with the Electron app, answered by the app's Rust side.
(() => {
  if (window.cm) return;
  const T = () => window.__TAURI_INTERNALS__;
  const fail = (e) => { throw new Error(typeof e === 'string' ? e : (e && e.message) || String(e)); };
  const call = (name) => (...args) => T().invoke('cm', { name, args: args.map((a) => (a === undefined ? null : a)) }).catch(fail);
  const b64 = (buf) => {
    const u = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
    let s = '';
    for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode.apply(null, u.subarray(i, i + 0x8000));
    return btoa(s);
  };
  const CHANNELS = ['usage', 'goto', 'stream', 'open-session', 'event', 'move-progress', 'update-available', 'update-progress', 'pending-projects', 'pair-requests', 'voice-install'];
  const names = {
    state: 'state', refresh: 'refresh', setSettings: 'settings:set', testNotify: 'notify:test', stats: 'stats', pairInfo: 'pair:info', newPairCode: 'pair:newcode',
    installHooks: 'hooks:install', removeHooks: 'hooks:remove', accent: 'system:accent', systemPalette: 'system:palette', systemDark: 'system:dark', setIcon: 'app:icon', setupPaths: 'setup:paths',
    installVoice: 'voice:install', setAutostart: 'autostart:set', pickFolder: 'pickFolder', openPath: 'openPath', moveProject: 'project:move', updateRun: 'update:run',
    updateCheck: 'update:check', updateLater: 'update:later', updateStatus: 'update:status', hubStatus: 'hub:status', phoneCode: 'hub:phonecode', continueHere: 'project:continue',
    stop: 'stop', openExternal: 'openExternal', openTerminal: 'terminal:open', machines: 'machines', addMachine: 'machines:add', removeMachine: 'machines:remove', discover: 'discover',
    notesSince: 'notes:since', relay: 'relay', guests: 'guests', setGuests: 'guests:set', shareJoin: 'share:join', send: 'send', voiceStatus: 'voice:status',
    callAction: 'call:action', supportState: 'support:state', supportStart: 'support:start', supportVerify: 'support:verify', supportLogout: 'support:logout', support: 'support:call',
    pairRequests: 'pair:requests', decidePair: 'pair:decide', relaunch: 'app:relaunch', version: 'app:version', pendingProjects: 'projects:pending', acceptProjects: 'projects:accept', rejectProjects: 'projects:reject',
  };
  const cm = {};
  for (const [k, v] of Object.entries(names)) cm[k] = call(v);
  cm.upload = (id, session, cwd, name, buf) => call('upload')(id, session, cwd, name, b64(buf));
  cm.stt = (id, wav) => call('voice:stt')(id, b64(wav));
  cm.tts = (id, text, lang) => T().invoke('cm_bin', { name: 'voice:tts', args: [id, text, lang === undefined ? null : lang] }).then((b) => new Uint8Array(b)).catch(fail);
  cm.filePath = () => '';   // files are uploaded into the project (also on this computer)
  cm.on = (ch, cb) => {
    if (!CHANNELS.includes(ch)) return () => {};
    let off = null, gone = false;
    window.__TAURI__.event.listen(ch, (e) => cb(e.payload)).then((u) => { if (gone) u(); else off = u; });
    return () => { gone = true; if (off) off(); };
  };
  window.cm = cm;
})();
