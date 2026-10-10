'use strict';
const { contextBridge, ipcRenderer, webUtils } = require('electron');

const inv = (ch) => (...a) => ipcRenderer.invoke(ch, ...a);
const CHANNELS = ['usage', 'goto', 'stream', 'open-session', 'event', 'move-progress', 'update-available', 'update-progress', 'pending-projects', 'pair-requests', 'voice-install'];

contextBridge.exposeInMainWorld('cm', {
  state: inv('state'), refresh: inv('refresh'),
  setSettings: inv('settings:set'),
  stats: inv('stats'), pairInfo: inv('pair:info'), newPairCode: inv('pair:newcode'), installHooks: inv('hooks:install'), removeHooks: inv('hooks:remove'),
  accent: inv('system:accent'), systemDark: inv('system:dark'), setIcon: inv('app:icon'), setupPaths: inv('setup:paths'), installVoice: inv('voice:install'), setAutostart: inv('autostart:set'), pickFolder: inv('pickFolder'), openPath: inv('openPath'), moveProject: inv('project:move'), updateRun: inv('update:run'), updateCheck: inv('update:check'), updateLater: inv('update:later'), updateStatus: inv('update:status'), hubStatus: inv('hub:status'), phoneCode: inv('hub:phonecode'), continueHere: inv('project:continue'), upload: inv('upload'), stop: inv('stop'), openExternal: inv('openExternal'),
  filePath: (f) => webUtils.getPathForFile(f),
  machines: inv('machines'), addMachine: inv('machines:add'), removeMachine: inv('machines:remove'), discover: inv('discover'),
  relay: inv('relay'), send: inv('send'),
  voiceStatus: inv('voice:status'), stt: inv('voice:stt'), tts: inv('voice:tts'),
  callAction: inv('call:action'),
  supportState: inv('support:state'), supportStart: inv('support:start'), supportVerify: inv('support:verify'), supportLogout: inv('support:logout'), support: inv('support:call'),
  pairRequests: inv('pair:requests'), decidePair: inv('pair:decide'), relaunch: inv('app:relaunch'), version: inv('app:version'), pendingProjects: inv('projects:pending'), acceptProjects: inv('projects:accept'), rejectProjects: inv('projects:reject'),
  on: (ch, cb) => {
    if (!CHANNELS.includes(ch)) return () => {};
    const f = (_e, d) => cb(d);
    ipcRenderer.on(ch, f);
    return () => ipcRenderer.removeListener(ch, f);
  },
});
