#!/usr/bin/env python3
"""Hearth relay: lets the Hearth phone app list and chat with the
Claude Code sessions stored on this machine. Stdlib only.

Run:  python3 claude_relay.py
"""
import hmac
import importlib.util
import io
import ipaddress
import json
import os
import secrets
import shutil
import socket
import subprocess
import threading
import time
import traceback
import wave
from collections import Counter, defaultdict
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Optional
from urllib.parse import parse_qs, urlparse

PORT: int = int(os.environ.get("CM_PORT", "47601"))
DISCOVER_PORT: int = 47600
PROJECTS: Path = Path.home() / ".claude" / "projects"
CFG: Path = Path.home() / ".config" / "hearth-relay"
CFG.mkdir(parents=True, exist_ok=True)
TOKEN_FILE: Path = CFG / "token"
if not TOKEN_FILE.exists():
    TOKEN_FILE.write_text(secrets.token_urlsafe(16))
    TOKEN_FILE.chmod(0o600)
TOKEN: str = TOKEN_FILE.read_text().strip()
CLAUDE: str = shutil.which("claude") or str(Path.home() / ".local/bin/claude")
NAME: str = socket.gethostname()
MODES = ("acceptEdits", "plan", "bypassPermissions", "default")
ACTIVE: dict[str, subprocess.Popen[str]] = {}  # turns started from the phone
CONFIG_FILE: Path = CFG / "config.json"
MIN_DONE_SECONDS: int = 20   # shorter turns don't ring
PERMISSION_WAIT: int = 25    # how long a permission prompt waits for the phone before falling back to the terminal


def load_config() -> dict[str, Any]:
    try:
        return json.loads(CONFIG_FILE.read_text())
    except (OSError, ValueError):
        return {}


def set_away(on: bool) -> None:
    CONFIG_FILE.write_text(json.dumps({**load_config(), "away": on}))


class Bus:
    """In-memory event feed the phone and desktop apps subscribe to."""

    def __init__(self) -> None:
        self.cv = threading.Condition()
        self.events: list[dict[str, Any]] = []
        self.seq = 0
        self.listeners = 0

    def publish(self, e: dict[str, Any]) -> None:
        with self.cv:
            self.seq += 1
            e["id"] = self.seq
            self.events.append(e)
            del self.events[:-100]
            self.cv.notify_all()

    def since(self, n: int, timeout: float) -> list[dict[str, Any]]:
        with self.cv:
            if not any(e["id"] > n for e in self.events):
                self.cv.wait(timeout)
            return [e for e in self.events if e["id"] > n]


BUS = Bus()
VOICE_DIR: Path = Path.home() / ".local/share/hearth-relay/voices"   # Piper (fallback)
MODEL_DIR: Path = Path.home() / ".local/share/hearth-relay/models"    # Kokoro
BRIEF_PROMPT = ("The user is listening to your reply read aloud. Keep it brief and conversational: short spoken "
                "sentences, no markdown, no lists, no code blocks. Say what you did and what you found.")
_stt_lock = threading.Lock()
_tts_lock = threading.Lock()
_load_lock = threading.Lock()
_models: dict[str, Any] = {}


def have(mod: str) -> bool:
    return importlib.util.find_spec(mod) is not None


def stt_engine() -> str:
    pref = load_config().get("stt_engine", "auto")
    if pref in ("auto", "parakeet") and have("onnx_asr"):
        return "parakeet"
    return "whisper" if have("faster_whisper") else ""


def tts_engine() -> str:
    pref = load_config().get("tts_engine", "auto")
    if pref in ("auto", "kokoro") and have("kokoro_onnx") and (MODEL_DIR / "kokoro-v1.0.onnx").exists():
        return "kokoro"
    return "piper" if have("piper") and next(VOICE_DIR.glob("*.onnx"), None) else ""


def voice_status() -> dict[str, Any]:
    s, t = stt_engine(), tts_engine()
    return {"stt": bool(s), "tts": bool(t), "stt_engine": s, "tts_engine": t, "dutch": have_dutch_voice(), "dutch_engine": "xtts" if nl_up() else "piper"}


def model(kind: str) -> Any:
    """Loads (once) and returns the speech model for the active engine."""
    with _load_lock:
        if kind in _models:
            return _models[kind]
        if kind == "parakeet":
            import onnx_asr  # type: ignore[import-not-found]
            _models[kind] = onnx_asr.load_model(load_config().get("stt_model_parakeet", "nemo-parakeet-tdt-0.6b-v3"), quantization="int8")  # v3: English + Dutch + 23 more, language detected automatically
        elif kind == "whisper":
            from faster_whisper import WhisperModel  # type: ignore[import-not-found]
            _models[kind] = WhisperModel(load_config().get("stt_model", "distil-large-v3"), device="cpu", compute_type="int8")
        elif kind == "kokoro":
            from kokoro_onnx import Kokoro  # type: ignore[import-not-found]
            _models[kind] = Kokoro(str(MODEL_DIR / "kokoro-v1.0.onnx"), str(MODEL_DIR / "voices-v1.0.bin"))
        else:
            from piper import PiperVoice  # type: ignore[import-not-found]
            _models[kind] = PiperVoice.load(str(next(VOICE_DIR.glob("en_*.onnx"), next(VOICE_DIR.glob("*.onnx")))))
        return _models[kind]


def transcribe(wav: bytes) -> str:
    """16 kHz mono PCM16 WAV -> text. Decoded here (not by PyAV) so it works with any PyAV version."""
    import numpy as np
    with wave.open(io.BytesIO(wav)) as w:
        audio = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768.0
    eng = stt_engine()
    with _stt_lock:
        if eng == "parakeet":
            return str(model("parakeet").recognize(audio)).strip()
        segs, _ = model("whisper").transcribe(audio, language="en", vad_filter=True, beam_size=1)
        return " ".join(x.text.strip() for x in segs).strip()


_NL = {"de", "het", "een", "en", "van", "ik", "je", "jij", "niet", "dat", "die", "op", "te", "met", "voor", "dit", "maar", "ook", "aan", "zijn", "heb", "hebben", "wat", "als", "naar", "bij", "kan", "wil", "nog", "uit", "dan", "er", "we", "wij", "ze", "hoe", "waar", "gaan", "maken", "hallo", "goed", "alles", "klaar", "gedaan", "bestand", "fout", "juist", "geen", "wordt", "deze", "heeft", "moet", "zou", "jouw", "mijn", "omdat", "want", "ja", "nee", "dank"}
_EN = {"the", "and", "of", "to", "that", "for", "with", "you", "it", "this", "are", "was", "have", "can", "will", "not", "but", "on", "in", "as", "be", "do", "what", "how", "we", "they", "from", "at", "your", "my", "hello", "done", "file", "error", "yes", "no", "thanks", "please", "should", "would", "which", "there", "been"}


def detect_lang(text: str, fallback: str = "en") -> str:
    import re
    words = [w for w in re.split(r"[^a-zà-ÿ']+", text.lower()) if w]
    nl, en = sum(w in _NL for w in words), sum(w in _EN for w in words)
    return "nl" if nl - en >= 2 else "en" if en - nl >= 2 else fallback


def piper_nl() -> Any:
    from piper import PiperVoice  # type: ignore[import-not-found]
    name = load_config().get("tts_voice_nl", "nl_BE-nathalie-medium")
    key = "piper-" + name
    if key not in _models:
        with _load_lock:
            _models[key] = PiperVoice.load(str(VOICE_DIR / f"{name}.onnx"))
    return _models[key]


def have_dutch_voice() -> bool:
    return (VOICE_DIR / f"{load_config().get('tts_voice_nl', 'nl_BE-nathalie-medium')}.onnx").exists() and have("piper")


def nl_up() -> bool:
    import urllib.request
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{os.environ.get('CM_NL_PORT', '47603')}/", timeout=0.5):
            return True
    except Exception:
        return False


def nl_remote(text: str) -> Optional[bytes]:
    """The better Dutch voice (XTTS, own process on 127.0.0.1:47603), when installed and running; else None."""
    import urllib.request
    port = os.environ.get("CM_NL_PORT", "47603")
    try:
        req = urllib.request.Request(f"http://127.0.0.1:{port}/tts", data=json.dumps({"text": text}).encode(), headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=90) as r:
            data = r.read()
        return data if data[:4] == b"RIFF" else None
    except Exception:
        return None


def synthesize(text: str, lang: str = "") -> bytes:
    """English: Kokoro (best quality). Dutch: Piper nl_BE. The language is the one given, else guessed from the words."""
    import numpy as np
    lang = lang if lang in ("nl", "en") else detect_lang(text)
    if lang == "nl":
        better = nl_remote(text)
        if better:
            return better
    buf = io.BytesIO()
    eng = tts_engine()
    with _tts_lock:
        if lang == "nl" and have_dutch_voice():
            with wave.open(buf, "wb") as w:
                v = piper_nl()
                (v.synthesize_wav if hasattr(v, "synthesize_wav") else v.synthesize)(text, w)
        elif eng == "kokoro":
            samples, rate = model("kokoro").create(text, voice=load_config().get("tts_voice", "af_heart"), speed=1.0, lang="en-us")
            pcm = (np.clip(samples, -1, 1) * 32767).astype(np.int16)
            with wave.open(buf, "wb") as w:
                w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate); w.writeframes(pcm.tobytes())
        else:
            with wave.open(buf, "wb") as w:
                v = model("piper")
                (v.synthesize_wav if hasattr(v, "synthesize_wav") else v.synthesize)(text, w)
    return buf.getvalue()


def warm_up() -> None:
    try:
        if stt_engine():
            model(stt_engine())
        if tts_engine():
            synthesize("ready")
        if have_dutch_voice():
            synthesize("klaar", "nl")
    except Exception:
        traceback.print_exc()


def text_of(content: Any) -> str:
    if isinstance(content, str):
        return content
    return "\n".join(b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text")


def read(path: Path) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    out.append(json.loads(line))
                except ValueError:
                    pass
    except OSError:
        pass
    return out


def find(sid: str) -> Optional[Path]:
    if not sid.replace("-", "").isalnum():
        return None
    return next(PROJECTS.glob(f"*/{sid}.jsonl"), None)


def summarize(path: Path) -> tuple[str, str]:
    title, cwd = "", ""
    for o in read(path):
        if o.get("type") == "ai-title":
            title = o.get("aiTitle") or title
        cwd = cwd or o.get("cwd") or ""
        if not title and o.get("type") == "user" and not o.get("isSidechain"):
            t = text_of(o.get("message", {}).get("content", "")).strip()
            if t and not t.startswith("<"):
                title = t[:80]
    return title or "Untitled", cwd


def open_claudes() -> dict[str, int]:
    """Working directory -> number of interactive Claude Code processes running there."""
    counts: dict[str, int] = {}
    for d in Path("/proc").iterdir():
        if not d.name.isdigit():
            continue
        try:
            if (d / "comm").read_text().strip() != "claude":
                continue
            if b"stream-json" in (d / "cmdline").read_bytes():
                continue  # a turn started by this relay, tracked in ACTIVE
            cwd = os.readlink(d / "cwd")
        except OSError:
            continue
        counts[cwd] = counts.get(cwd, 0) + 1
    return counts


def sessions() -> list[dict[str, Any]]:
    files = sorted(PROJECTS.glob("*/*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)[:40]
    now = time.time()
    open_by_cwd = open_claudes()
    res = []
    for p in files:
        title, cwd = summarize(p)
        age = now - p.stat().st_mtime
        # the N most recently written sessions in a folder belong to the N claudes open there
        live = p.stem in ACTIVE or open_by_cwd.get(cwd, 0) > 0
        if live and p.stem not in ACTIVE:
            open_by_cwd[cwd] -= 1
        res.append({"id": p.stem, "title": title, "cwd": cwd, "mtime": int(p.stat().st_mtime * 1000),
                    "live": live, "busy": age < 20 or p.stem in ACTIVE})
    return res


def clip(v: Any, n: int) -> str:
    return str(v)[:n]


def tool_input(name: str, arg: dict[str, Any]) -> dict[str, Any]:
    """Trimmed tool input for display: enough to show a command, a file, or a diff."""
    if name in ("Edit", "NotebookEdit"):
        return {"file_path": arg.get("file_path", ""), "old_string": clip(arg.get("old_string", ""), 1500), "new_string": clip(arg.get("new_string", ""), 1500)}
    if name == "MultiEdit":
        return {"file_path": arg.get("file_path", ""), "edits": [{"old_string": clip(e.get("old_string", ""), 800), "new_string": clip(e.get("new_string", ""), 800)} for e in arg.get("edits", [])[:6]]}
    if name == "Write":
        return {"file_path": arg.get("file_path", ""), "content": clip(arg.get("content", ""), 1500)}
    if name == "Bash":
        return {"command": clip(arg.get("command", ""), 1500), "description": clip(arg.get("description", ""), 200)}
    return {k: clip(v, 300) for k, v in list(arg.items())[:6]}


def result_text(c: Any) -> str:
    if isinstance(c, list):
        c = "\n".join(x.get("text", "") for x in c if isinstance(x, dict))
    return clip(c, 2000)


def tool_line(b: dict[str, Any]) -> str:
    arg = b.get("input", {})
    hint = next((str(arg[k])[:60] for k in ("command", "file_path", "pattern", "path", "url") if k in arg), "")
    return f"{b.get('name')} {hint}".strip()


def messages(sid: str, tail: int = 80) -> list[dict[str, Any]]:
    p = find(sid)
    out: list[dict[str, Any]] = []
    if not p:
        return out
    entries = read(p)
    results: dict[str, dict[str, Any]] = {}
    for o in entries:
        c = o.get("message", {}).get("content", "")
        if o.get("type") == "user" and isinstance(c, list):
            for blk in c:
                if blk.get("type") == "tool_result":
                    results[blk.get("tool_use_id", "")] = {"result": result_text(blk.get("content")), "error": bool(blk.get("is_error"))}
    for o in entries:
        if o.get("isSidechain") or o.get("type") not in ("user", "assistant"):
            continue
        c = o.get("message", {}).get("content", "")
        ts = int(iso_ts(o) * 1000)
        if o["type"] == "user":
            t = text_of(c).strip()
            if t and not t.startswith("<"):
                out.append({"role": "user", "text": t, "ts": ts})
        elif isinstance(c, list):
            for blk in c:
                if blk.get("type") == "text" and blk.get("text", "").strip():
                    out.append({"role": "assistant", "text": blk["text"].strip(), "ts": ts})
                elif blk.get("type") == "tool_use":
                    name, arg = blk.get("name", ""), blk.get("input", {})
                    out.append({"role": "tool", "text": tool_line(blk), "name": name, "input": tool_input(name, arg), "ts": ts,
                                **results.get(blk.get("id", ""), {})})
    return out[-tail:]


def lan_only(addr: str) -> bool:
    """Accept only loopback, link-local and private (RFC 1918) addresses."""
    try:
        ip = ipaddress.ip_address(addr.split("%")[0])
    except ValueError:
        return False
    return ip.is_loopback or ip.is_link_local or ip.is_private


def turn_info(path: Optional[Path]) -> tuple[int, str]:
    """Seconds since the user's last prompt, and Claude's last text reply."""
    started, last = 0.0, ""
    if path:
        for o in read(path):
            if o.get("isSidechain"):
                continue
            c = o.get("message", {}).get("content", "")
            if o.get("type") == "user" and text_of(c).strip():
                try:
                    started = datetime.fromisoformat(o["timestamp"].replace("Z", "+00:00")).timestamp()
                except (KeyError, ValueError):
                    pass
            elif o.get("type") == "assistant" and isinstance(c, list) and text_of(c).strip():
                last = text_of(c).strip()
    return (int(time.time() - started) if started else 0), last


def describe(tool: str, arg: dict[str, Any]) -> str:
    hint = next((str(arg[k])[:120] for k in ("command", "file_path", "pattern", "path", "url") if k in arg), "")
    return f"{tool}: {hint}".strip(": ")


def handle_hook(h: dict[str, Any]) -> dict[str, Any]:
    """Called by cm_hook.py from Claude Code hooks. Only acts while away mode is on."""
    if not load_config().get("away") or BUS.listeners == 0:
        return {}
    name, sid, tool = h.get("hook_event_name"), h.get("session_id", ""), h.get("tool_name", "")
    path = find(sid)
    title, cwd = summarize(path) if path else ("Claude Code", "")
    base = {"session": sid, "title": title, "cwd": cwd or h.get("cwd", ""), "machine": NAME,
            "ts": int(time.time() * 1000)}
    if name == "Stop":
        secs, last = turn_info(path)
        if secs >= MIN_DONE_SECONDS:
            BUS.publish({**base, "kind": "done", "text": last[:200], "seconds": secs})
    elif tool == "AskUserQuestion" and name in ("PreToolUse", "PermissionRequest"):
        if time.time() - LAST_QUESTION.get(sid, 0) > 10:
            LAST_QUESTION[sid] = time.time()
            qs = h.get("tool_input", {}).get("questions") or [{}]
            BUS.publish({**base, "kind": "question", "text": str(qs[0].get("question", "Claude has a question"))[:200]})
    elif name == "PermissionRequest":
        req = secrets.token_hex(4)
        entry: dict[str, Any] = {"ev": threading.Event(), "behavior": None}
        PENDING[req] = entry
        BUS.publish({**base, "kind": "permission", "req": req, "tool": tool, "text": describe(tool, h.get("tool_input", {}))})
        entry["ev"].wait(PERMISSION_WAIT)
        PENDING.pop(req, None)
        if entry["behavior"] in ("allow", "deny"):
            return {"decision": entry["behavior"]}
    return {}


_STATS_CACHE: dict[str, tuple[float, int, dict[str, Any]]] = {}


def iso_ts(o: dict[str, Any]) -> float:
    try:
        return datetime.fromisoformat(o["timestamp"].replace("Z", "+00:00")).timestamp()
    except (KeyError, ValueError):
        return 0.0


def parse_usage_file(p: Path) -> dict[str, Any]:
    """Per-session usage facts. Claude Code logs a message once per content block, so de-duplicate by message id."""
    msgs: dict[str, dict[str, Any]] = {}
    users: list[float] = []
    tool_ids: dict[str, str] = {}
    cwd = ""
    for o in read(p):
        cwd = cwd or o.get("cwd") or ""
        kind = o.get("type")
        if kind == "assistant":
            m = o.get("message", {})
            u = m.get("usage") or {}
            model = m.get("model", "")
            if not u or model.startswith("<"):
                continue
            rec = {"ts": iso_ts(o), "model": model, "in": u.get("input_tokens", 0), "out": u.get("output_tokens", 0),
                   "cr": u.get("cache_read_input_tokens", 0), "cw": u.get("cache_creation_input_tokens", 0)}
            key = m.get("id") or o.get("uuid", "")
            if key not in msgs or rec["out"] >= msgs[key]["out"]:
                msgs[key] = rec
            for b in m.get("content", []) if isinstance(m.get("content"), list) else []:
                if b.get("type") == "tool_use":
                    tool_ids[b.get("id", "")] = b.get("name", "")
        elif kind == "user" and not o.get("isSidechain") and text_of(o.get("message", {}).get("content", "")).strip():
            users.append(iso_ts(o))
    return {"cwd": cwd, "msgs": list(msgs.values()), "users": users, "tools": list(tool_ids.values())}


def family(model: str) -> str:
    return next((f for f in ("opus", "sonnet", "haiku") if f in model), "other")


def stats(days: int) -> dict[str, Any]:
    now = time.time()
    cutoff = now - days * 86400
    per_day: dict[str, dict[str, Any]] = defaultdict(lambda: {"in": 0, "out": 0, "cr": 0, "cw": 0, "msgs": 0, "fam": Counter(), "sess": set()})
    projects: dict[str, dict[str, Any]] = defaultdict(lambda: {"tokens": 0, "sessions": set(), "last": 0.0})
    models: Counter[str] = Counter()
    tools: Counter[str] = Counter()
    heat = [[0] * 24 for _ in range(7)]
    day = lambda ts: datetime.fromtimestamp(ts).strftime("%Y-%m-%d")  # noqa: E731
    for p in PROJECTS.glob("*/*.jsonl"):
        try:
            st = p.stat()
        except OSError:
            continue
        if st.st_mtime < cutoff:
            continue
        hit = _STATS_CACHE.get(str(p))
        if hit and hit[0] == st.st_mtime and hit[1] == st.st_size:
            d = hit[2]
        else:
            d = parse_usage_file(p)
            _STATS_CACHE[str(p)] = (st.st_mtime, st.st_size, d)
        proj = projects[d["cwd"] or "unknown"]
        for m in d["msgs"]:
            if m["ts"] < cutoff:
                continue
            row = per_day[day(m["ts"])]
            for k in ("in", "out", "cr", "cw"):
                row[k] += m[k]
            work = m["in"] + m["out"] + m["cw"]
            row["fam"][family(m["model"])] += work
            row["sess"].add(p.stem)
            proj["tokens"] += work
            proj["sessions"].add(p.stem)
            proj["last"] = max(proj["last"], m["ts"])
            models[family(m["model"])] += work
        for ts in d["users"]:
            if ts >= cutoff:
                per_day[day(ts)]["msgs"] += 1
                dt = datetime.fromtimestamp(ts)
                heat[dt.weekday()][dt.hour] += 1
        if d["msgs"] and d["msgs"][-1]["ts"] >= cutoff:
            tools.update(d["tools"])
    out_days = []
    for i in range(days - 1, -1, -1):
        k = day(now - i * 86400)
        r = per_day.get(k)
        out_days.append({"date": k, "in": r["in"] if r else 0, "out": r["out"] if r else 0, "cr": r["cr"] if r else 0, "cw": r["cw"] if r else 0,
                         "msgs": r["msgs"] if r else 0, "sessions": len(r["sess"]) if r else 0, "fam": dict(r["fam"]) if r else {}})
    top = sorted(projects.items(), key=lambda kv: kv[1]["tokens"], reverse=True)[:10]
    return {"generated": int(now * 1000), "days": out_days, "models": dict(models), "tools": dict(tools.most_common(10)), "heat": heat,
            "projects": [{"cwd": c, "tokens": v["tokens"], "sessions": len(v["sessions"]), "last": int(v["last"] * 1000)} for c, v in top if v["tokens"]]}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args: Any) -> None:
        pass

    def authed(self) -> bool:
        if not lan_only(self.client_address[0]):
            self.reply(403, {"error": "local network only"})
            return False
        if hmac.compare_digest(self.headers.get("Authorization", ""), f"Bearer {TOKEN}"):
            return True
        self.reply(401, {"error": "bad token"})
        return False

    def reply(self, code: int, obj: Any) -> None:
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self) -> None:
        parts = urlparse(self.path).path.strip("/").split("/")
        if parts == ["stats"] and self.authed():
            q = parse_qs(urlparse(self.path).query)
            try:
                n = max(1, min(90, int(q.get("days", ["30"])[0])))
            except ValueError:
                n = 30
            return self.reply(200, stats(n))
        if parts == ["voice", "status"] and self.authed():
            return self.reply(200, voice_status())
        if parts == ["status"] and self.authed():
            return self.reply(200, {"away": bool(load_config().get("away")), "listeners": BUS.listeners})
        if parts == ["events"] and self.authed():
            return self.events()
        if parts == ["info"] and lan_only(self.client_address[0]):
            return self.reply(200, {"name": NAME, "claude": os.path.exists(CLAUDE)})
        if not self.authed():
            return
        if parts == ["sessions"]:
            return self.reply(200, sessions())
        if len(parts) == 3 and parts[0] == "sessions" and parts[2] == "messages":
            q = parse_qs(urlparse(self.path).query)
            try:
                n = max(1, min(1000, int(q.get("tail", ["80"])[0])))
            except ValueError:
                n = 80
            return self.reply(200, messages(parts[1], n))
        self.reply(404, {"error": "not found"})

    def events(self) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        last = BUS.seq
        BUS.listeners += 1
        try:
            self.wfile.write(b": hello\n\n")
            self.wfile.flush()
            while True:
                evs = BUS.since(last, 15)
                for e in evs:
                    self.wfile.write(f"data: {json.dumps(e)}\n\n".encode())
                    last = e["id"]
                if not evs:
                    self.wfile.write(b": ping\n\n")
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError, OSError):
            pass
        finally:
            BUS.listeners -= 1
            self.close_connection = True

    def post_json(self) -> dict[str, Any]:
        try:
            return json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        except ValueError:
            return {}

    def do_POST(self) -> None:
        parts = urlparse(self.path).path.strip("/").split("/")
        if parts == ["hook"]:  # from Claude Code on this machine only
            if not ipaddress.ip_address(self.client_address[0].split("%")[0]).is_loopback:
                return self.reply(403, {"error": "loopback only"})
            return self.reply(200, handle_hook(self.post_json()))
        if parts == ["voice", "stt"]:
            if not self.authed():
                return
            n = int(self.headers.get("Content-Length", 0))
            if n > 10_000_000 or not voice_status()["stt"]:
                return self.reply(400, {"error": "speech-to-text unavailable"})
            try:
                return self.reply(200, {"text": transcribe(self.rfile.read(n))})
            except Exception as e:
                traceback.print_exc()
                return self.reply(500, {"error": f"{type(e).__name__}: {e}"[:200]})
        if parts == ["voice", "tts"]:
            if not self.authed():
                return
            body = self.post_json()
            text = str(body.get("text", ""))[:1000]
            if not text.strip() or not voice_status()["tts"]:
                return self.reply(400, {"error": "text-to-speech unavailable"})
            try:
                wav = synthesize(text, str(body.get("lang", "")))
            except Exception as e:
                traceback.print_exc()
                return self.reply(500, {"error": f"{type(e).__name__}: {e}"[:200]})
            self.send_response(200)
            self.send_header("Content-Type", "audio/wav")
            self.send_header("Content-Length", str(len(wav)))
            self.end_headers()
            self.wfile.write(wav)
            return
        if parts == ["call"]:  # explicit "ring my phone" from a script on this machine; ignores away mode
            if not ipaddress.ip_address(self.client_address[0].split("%")[0]).is_loopback:
                return self.reply(403, {"error": "loopback only"})
            b = self.post_json()
            sid, cwd = str(b.get("session", "")), str(b.get("cwd", ""))
            if not sid and cwd:  # newest session in that folder
                sid = next((x["id"] for x in sessions() if x["cwd"] == cwd), "")
            title = summarize(find(sid))[0] if sid and find(sid) else "Claude Code"
            BUS.publish({"kind": "done", "session": sid, "title": title, "cwd": cwd, "machine": NAME,
                         "ts": int(time.time() * 1000), "text": str(b.get("text", "Claude is calling"))[:200], "seconds": 0})
            return self.reply(200, {"listeners": BUS.listeners})
        if parts in (["away"], ["decision"]):
            if not self.authed():
                return
            b = self.post_json()
            if parts == ["away"]:
                set_away(bool(b.get("on")))
            elif b.get("req") in PENDING:
                PENDING[b["req"]]["behavior"] = b.get("behavior")
                PENDING[b["req"]]["ev"].set()
            return self.reply(200, {"ok": True})
        if not self.authed():
            return
        if parts == ["stop"]:
            if not self.authed():
                return
            proc = ACTIVE.get(str(self.post_json().get("session", "")))
            if proc:
                proc.terminate()
            return self.reply(200, {"ok": bool(proc)})
        if not (len(parts) == 3 and parts[0] == "sessions" and parts[2] == "send"):
            return self.reply(404, {"error": "not found"})
        sid = parts[1]
        body = self.post_json()
        mode = body.get("mode", "acceptEdits")
        mode = mode if mode in MODES else "acceptEdits"
        cmd = [CLAUDE, "-p", body.get("text", ""), "--output-format", "stream-json", "--verbose", "--include-partial-messages",
               "--permission-mode", mode]
        if body.get("model") in ("opus", "sonnet", "haiku"):
            cmd += ["--model", body["model"]]
        if body.get("brief"):
            cmd += ["--append-system-prompt", BRIEF_PROMPT]
        if sid == "new":  # start a fresh Claude Code session in a folder
            cwd = str(body.get("cwd", ""))
            if not os.path.isdir(cwd):
                return self.reply(400, {"error": "that folder doesn't exist on this computer"})
            key = "new-" + secrets.token_hex(4)
        else:
            p = find(sid)
            if not p:
                return self.reply(404, {"error": "no such session"})
            if sid in ACTIVE:
                return self.reply(409, {"error": "a turn is already running"})
            cwd = summarize(p)[1] or str(Path.home())
            cmd += ["--resume", sid]
            key = sid
        self.send_response(200)
        self.send_header("Content-Type", "application/x-ndjson")
        self.send_header("Connection", "close")
        self.end_headers()
        self.stream(cmd, cwd, key)
        self.close_connection = True

    def emit(self, o: dict[str, Any]) -> None:
        self.wfile.write((json.dumps(o) + "\n").encode())
        self.wfile.flush()

    def stream(self, cmd: list[str], cwd: str, sid: str) -> None:
        try:
            proc = subprocess.Popen(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                    stdin=subprocess.DEVNULL, text=True)
            ACTIVE[sid] = proc
            assert proc.stdout is not None
            for line in proc.stdout:
                try:
                    o = json.loads(line)
                except ValueError:
                    continue
                t = o.get("type")
                if t == "system" and o.get("subtype") == "init":
                    real = o.get("session_id", "")
                    if real and real != sid:
                        ACTIVE.pop(sid, None)
                        sid = real
                        ACTIVE[sid] = proc
                    self.emit({"t": "session", "id": real, "model": o.get("model", "")})
                elif t == "stream_event":
                    d = o.get("event", {}).get("delta", {})
                    if d.get("type") == "text_delta":
                        self.emit({"t": "delta", "text": d["text"]})
                elif t == "assistant":
                    for b in o.get("message", {}).get("content", []):
                        if b.get("type") == "tool_use":
                            n, arg = b.get("name", ""), b.get("input", {})
                            self.emit({"t": "tool", "id": b.get("id", ""), "name": n, "input": tool_input(n, arg), "text": tool_line(b)})
                elif t == "user":
                    c = o.get("message", {}).get("content", [])
                    for b in c if isinstance(c, list) else []:
                        if b.get("type") == "tool_result":
                            self.emit({"t": "result", "id": b.get("tool_use_id", ""), "text": result_text(b.get("content")), "error": bool(b.get("is_error"))})
                elif t == "result":
                    self.emit({"t": "done", "error": bool(o.get("is_error")), "ms": o.get("duration_ms"), "cost": o.get("total_cost_usd")})
            proc.wait()
        except (BrokenPipeError, ConnectionResetError):
            proc = ACTIVE.get(sid)
            if proc:
                proc.terminate()
        except OSError as e:
            self.emit({"t": "done", "error": True, "text": str(e)})
        finally:
            ACTIVE.pop(sid, None)


def discovery() -> None:
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("", DISCOVER_PORT))
    while True:
        data, addr = s.recvfrom(256)
        if data.startswith(b"CLAUDE_METER_DISCOVER") and lan_only(addr[0]):
            s.sendto(json.dumps({"name": NAME, "port": PORT}).encode(), addr)


def main() -> None:
    sidecar = os.environ.get("CM_SIDECAR") == "1"  # voice engine only: the main relay forwards /voice/* here
    if not sidecar:
        threading.Thread(target=discovery, daemon=True).start()
    threading.Thread(target=warm_up, daemon=True).start()
    print(f"Hearth {'voice engine' if sidecar else 'relay'} on port {PORT}")
    ThreadingHTTPServer(("127.0.0.1" if sidecar else "0.0.0.0", PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
