'use strict';
/* Voice conversation: your microphone -> relay (faster-whisper) -> Claude -> relay (Piper) -> speakers. */
const WORKLET = `class Cap extends AudioWorkletProcessor { process(i) { const c = i[0][0]; if (c) this.port.postMessage(c.slice(0)); return true; } }
registerProcessor('cap', Cap);`;

function toWav(frames) {
  let n = 0; for (const f of frames) n += f.length;
  const buf = new ArrayBuffer(44 + n * 2), v = new DataView(buf);
  const str = (o, s) => { for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i)); };
  str(0, 'RIFF'); v.setUint32(4, 36 + n * 2, true); str(8, 'WAVEfmt '); v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
  v.setUint32(24, 16000, true); v.setUint32(28, 32000, true); v.setUint16(32, 2, true); v.setUint16(34, 16, true); str(36, 'data'); v.setUint32(40, n * 2, true);
  let o = 44;
  for (const f of frames) for (let i = 0; i < f.length; i++, o += 2) v.setInt16(o, Math.max(-1, Math.min(1, f[i])) * 32767, true);
  return buf;
}

/** Splits streamed text into whole sentences; the unfinished tail stays in the returned rest. */
function takeSentences(buf) {
  const out = [];
  for (;;) {
    const m = /([.!?]+["')\]]*\s+|\n+)/.exec(buf);
    if (!m) break;
    const end = m.index + m[0].length;
    if (end >= buf.length && !m[0].includes('\n')) break;
    out.push(buf.slice(0, end).trim());
    buf = buf.slice(end);
  }
  return { sentences: out.filter(Boolean), rest: buf };
}
const NL = new Set('de het een en van ik je jij niet dat die op te met voor dit maar ook aan zijn heb hebben wat als naar bij kan wil nog uit dan er we wij ze hoe waar gaan maken hallo goed alles klaar gedaan bestand fout juist geen wordt deze heeft moet zou jouw mijn omdat want ja nee dank'.split(' '));
const EN = new Set('the and of to that for with you it this are was have can will not but on in as be do what how we they from at your my hello done file error yes no thanks please should would which there been'.split(' '));
/** "nl" or "en" guessed from the words; the fallback decides when it isn't clear. */
function detectLang(s, fallback) {
  const w = s.toLowerCase().split(/[^a-zà-ÿ']+/).filter(Boolean);
  const nl = w.filter((x) => NL.has(x)).length, en = w.filter((x) => EN.has(x)).length;
  return nl - en >= 2 ? 'nl' : en - nl >= 2 ? 'en' : fallback;
}
const spoken = (s) => s.replace(/```[\s\S]*?```/g, ' code omitted. ').replace(/[*_`#>]+/g, '').replace(/https?:\/\/\S+/g, 'a link').trim();

class Voice {
  constructor(host, mid, sess, mode, hooks) {
    Object.assign(this, { host, mid, sess, mode, hooks, lang: localStorage.getItem('cm.voiceLang') || ((navigator.language || '').startsWith('nl') ? 'nl' : 'en'), phase: 'idle', handsFree: true, lines: [], queue: [], pending: 0, pumpId: 0, muted: false, closed: false, running: false });
  }

  async open() {
    const el = this.el = document.createElement('div');
    el.className = 'voice';
    el.innerHTML = `<div class="row" style="width:100%"><div class="grow"><h3></h3><div class="small muted engine"></div></div><button class="ghost end">End</button></div>
      <div class="orb" tabindex="0" role="button" aria-label="Talk">${sparkSVG(160)}</div>
      <div class="status muted"></div><div class="small err errmsg" style="margin-top:6px"></div>
      <div class="transcript"></div>
      <div class="row" style="margin-top:12px"><button class="chip on hf">Hands-free</button><button class="chip lg"></button></div>`;
    this.host.append(el);
    el.querySelector('h3').textContent = this.sess.title;
    this.orb = el.querySelector('.orb');
    el.querySelector('.end').onclick = () => this.close();
    this.orb.onclick = () => this.tap();
    const lg = el.querySelector('.lg');
    const showLang = () => { lg.textContent = this.lang === 'nl' ? 'Nederlands' : 'English'; };
    showLang();
    lg.onclick = () => { this.lang = this.lang === 'nl' ? 'en' : 'nl'; localStorage.setItem('cm.voiceLang', this.lang); showLang(); };
    el.querySelector('.hf').onclick = (e) => { this.handsFree = !this.handsFree; e.target.classList.toggle('on', this.handsFree); };
    this.setPhase('idle');
    try {
      const st = await cm.voiceStatus(this.mid);
      if (!st.stt || !st.tts) throw new Error('Local voice isn\'t installed on this computer. Run relay/install_voice.sh.');
      el.querySelector('.engine').textContent = 'Local voice · Whisper + Piper';
      this.stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, channelCount: 1 } });
      this.ctx = new AudioContext({ sampleRate: 16000 });
      await this.ctx.audioWorklet.addModule(URL.createObjectURL(new Blob([WORKLET], { type: 'application/javascript' })));
      this.node = new AudioWorkletNode(this.ctx, 'cap');
      this.ctx.createMediaStreamSource(this.stream).connect(this.node);
    } catch (e) { this.error(e.message || String(e)); return; }
    this.begin();
  }

  error(m) { if (this.el) this.el.querySelector('.errmsg').textContent = m || ''; }
  setPhase(p) {
    this.phase = p;
    if (!this.el) return;
    this.orb.className = 'orb' + (p === 'thinking' ? ' thinking' : p === 'speaking' ? ' speaking' : '');
    this.orb.style.transform = '';
    this.el.querySelector('.status').textContent = { idle: 'Tap to talk', listening: 'Listening…', thinking: this.status || 'Thinking…', speaking: 'Speaking · tap to interrupt' }[p];
  }
  setLevel(l) { if (this.phase === 'listening') this.orb.style.transform = `scale(${1 + l * 0.4})`; }
  addLine(you, text) {
    this.lines.push([you, text]);
    const t = this.el.querySelector('.transcript');
    t.innerHTML = '';
    for (const [y, x] of this.lines.slice(-4)) { const d = document.createElement('div'); d.className = y ? 'small muted' : ''; d.textContent = x; t.append(d); }
    t.scrollTop = t.scrollHeight;
  }

  tap() {
    if (!this.node) return;
    if (this.phase === 'idle') this.begin();
    else if (this.phase === 'listening') this.cancelListen && this.cancelListen();
    else if (this.phase === 'speaking') { this.muted = true; this.resetSpeech(); }
  }
  begin() { if (!this.running) { this.running = true; this.converse().finally(() => { this.running = false; if (!this.closed) this.setPhase('idle'); }); } }

  listen() {
    return new Promise((resolve) => {
      const frames = []; let i = 0, floor = 0, first = -1, last = -1;
      const finish = (fr) => { this.node.port.onmessage = null; this.cancelListen = null; resolve(fr); };
      this.cancelListen = () => finish(null);
      this.node.port.onmessage = (e) => {
        const f = e.data; let s = 0;
        for (let k = 0; k < f.length; k++) s += f[k] * f[k];
        const rms = Math.sqrt(s / f.length);
        this.setLevel(Math.min(1, rms * 12));
        frames.push(f);
        if (i < 37) floor = Math.max(floor, rms);
        else if (rms > Math.max(floor * 2.5, 0.02)) { if (first < 0) first = i; last = i; }
        i++;
        if (first < 0 && i > 1000) return finish(null);
        if ((first >= 0 && i - last > 112 && last - first > 40) || i > 3750) finish(frames.slice(Math.max(0, first - 37)));
      };
    });
  }

  async converse() {
    let empty = 0;
    while (!this.closed) {
      this.error(''); this.status = '';
      this.setPhase('listening');
      const frames = await this.listen();
      if (this.closed) return;
      let said = null;
      if (frames) {
        this.setPhase('thinking'); this.status = 'Transcribing…'; this.setPhase('thinking');
        try { said = (await cm.stt(this.mid, toWav(frames))).trim(); } catch (e) { this.error(String(e.message || e).replace(/^Error invoking remote method '[^']+': (Error: )?/, '')); return; }
      }
      if (!said) { if (++empty >= 2 || !this.handsFree) return; continue; }
      empty = 0;
      this.addLine(true, said);
      this.status = ''; this.setPhase('thinking'); this.muted = false; this.resetSpeech();
      const ok = await this.ask(said);
      this.flushTail();
      await this.drain();
      if (!ok || !this.handsFree) return;
    }
  }

  ask(text) {
    return new Promise((resolve) => {
      const sid = Math.random().toString(36).slice(2);
      let buf = '', reply = '';
      this.tail = () => buf; this.clearTail = () => { buf = ''; };
      voiceHandlers[sid] = (ev) => {
        if (ev.t === 'delta') {
          reply += ev.text; buf += ev.text;
          const r = takeSentences(buf); buf = r.rest;
          for (const s of r.sentences) if (!this.muted) { if (this.phase !== 'speaking') this.setPhase('speaking'); this.enqueue(spoken(s)); }
        } else if (ev.t === 'tool') { this.status = 'Working: ' + ev.text; if (this.phase === 'thinking') this.setPhase('thinking'); }
        else if (ev.t === 'end' || ev.t === 'fail') {
          delete voiceHandlers[sid];
          if (ev.t === 'fail') this.error(ev.text);
          if (reply.trim()) this.addLine(false, reply.trim());
          resolve(ev.t === 'end');
        }
      };
      cm.send(this.mid, this.sess.id, text, this.mode, false, sid, { model: 'haiku' }).catch((e) => { delete voiceHandlers[sid]; this.error(String(e.message || e)); resolve(false); });
    });
  }
  flushTail() { const t = this.tail ? this.tail() : ''; if (t.trim() && !this.muted) { this.setPhase('speaking'); this.enqueue(spoken(t)); } if (this.clearTail) this.clearTail(); }

  enqueue(text) {
    if (!text.trim()) return;
    this.pending++;
    this.queue.push(cm.tts(this.mid, text, detectLang(text, this.lang)).catch(() => null));
    this.pump();
  }
  async pump() {
    if (this.pumping) return;
    this.pumping = true;
    const id = ++this.pumpId;
    while (this.queue.length && id === this.pumpId) {
      const buf = await this.queue.shift();
      if (id !== this.pumpId) return;
      if (buf) await this.play(buf);
      if (id !== this.pumpId) return;
      this.pending--;
    }
    if (id === this.pumpId) this.pumping = false;
  }
  play(buf) {
    return new Promise((res) => {
      const url = URL.createObjectURL(new Blob([buf], { type: 'audio/wav' }));
      const a = this.audio = new Audio(url);
      a.onended = a.onerror = () => { URL.revokeObjectURL(url); res(); };
      a.play().catch(() => res());
    });
  }
  resetSpeech() {
    this.pumpId++; this.pumping = false; this.queue = []; this.pending = 0;
    if (this.audio) { this.audio.pause(); this.audio = null; }
  }
  drain() { return new Promise((res) => { const t = setInterval(() => { if (this.pending <= 0 || this.closed) { clearInterval(t); res(); } }, 80); }); }

  close() {
    if (this.closed) return;
    this.closed = true;
    this.resetSpeech();
    if (this.cancelListen) this.cancelListen();
    if (this.stream) this.stream.getTracks().forEach((t) => t.stop());
    if (this.ctx) this.ctx.close();
    if (this.el) this.el.remove();
    this.hooks.onClose();
  }
}
const voiceHandlers = {};
