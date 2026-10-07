#!/usr/bin/env python3
"""Dutch speech from Coqui XTTS-v2 (natural, multilingual) on 127.0.0.1:47603. POST /tts {"text": "..."} -> audio/wav.
Runs in its own environment (install_dutch_voice.sh) so its heavy dependencies never touch the main voice engine."""
import io
import json
import os
import threading
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Optional

PORT: int = int(os.environ.get("CM_NL_PORT", "47603"))
SPEAKER: str = os.environ.get("CM_NL_SPEAKER", "Ana Florence")
_lock = threading.Lock()
_tts: Optional[Any] = None


def model() -> Any:
    global _tts
    if _tts is None:
        import torch
        from TTS.api import TTS  # type: ignore[import-not-found]
        torch.set_num_threads(max(2, (os.cpu_count() or 4) - 2))
        _tts = TTS("tts_models/multilingual/multi-dataset/xtts_v2")
    return _tts


def speak(text: str) -> bytes:
    import numpy as np
    with _lock:
        samples = model().tts(text=text, speaker=SPEAKER, language="nl")
    pcm = (np.clip(np.asarray(samples, dtype=np.float32), -1, 1) * 32767).astype(np.int16)
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(24000)
        w.writeframes(pcm.tobytes())
    return buf.getvalue()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args: Any) -> None:
        pass

    def do_GET(self) -> None:
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ok")

    def do_POST(self) -> None:
        try:
            text = str(json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}").get("text", ""))[:800]
            wav = speak(text) if text.strip() else b""
        except Exception as e:  # noqa: BLE001
            body = json.dumps({"error": str(e)[:200]}).encode()
            self.send_response(500)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav)))
        self.end_headers()
        self.wfile.write(wav)


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
