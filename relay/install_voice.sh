#!/usr/bin/env bash
# Installs the local voice engines for the relay, all CPU-only:
#   speech-to-text:  NVIDIA Parakeet TDT 0.6B v3 (int8, via onnx-asr)  ~700 MB, English + Dutch + 23 more languages
#   text-to-speech:  Kokoro 82M for English (~340 MB) and Piper nl_BE Nathalie for Dutch (~60 MB)
#   fallbacks:       faster-whisper and Piper (used only if the above are missing)
# Safe to re-run. When it finishes the relay service restarts using the new environment.
set -euo pipefail
BASE="$HOME/.local/share/hearth-relay"
VENV="$BASE/venv"
MODELS="$BASE/models"
VOICES="$BASE/voices"
UNIT="$HOME/.config/systemd/user/hearth-relay.service"
KOKORO="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"
PIPER="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium"

mkdir -p "$BASE" "$MODELS" "$VOICES"
command -v espeak-ng >/dev/null || echo "Note: Kokoro needs espeak-ng (sudo dnf install espeak-ng)."
[ -d "$VENV" ] || python3 -m venv "$VENV"
"$VENV/bin/pip" install --quiet --upgrade pip
"$VENV/bin/pip" install --quiet kokoro-onnx "onnx-asr[cpu,hub]" faster-whisper piper-tts

for f in kokoro-v1.0.onnx voices-v1.0.bin; do
  [ -s "$MODELS/$f" ] || curl -fL --progress-bar -o "$MODELS/$f" "$KOKORO/$f"
done
for f in en_US-lessac-medium.onnx en_US-lessac-medium.onnx.json; do
  [ -s "$VOICES/$f" ] || curl -fL --progress-bar -o "$VOICES/$f" "$PIPER/$f"
done

for f in nl_BE-nathalie-medium.onnx nl_BE-nathalie-medium.onnx.json; do
  [ -s "$VOICES/$f" ] || curl -fL --progress-bar -o "$VOICES/$f" "https://huggingface.co/rhasspy/piper-voices/resolve/main/nl/nl_BE/nathalie/medium/$f"
done

echo "Downloading the speech recognition model (first run only, ~700 MB)..."
"$VENV/bin/python" - <<'PY'
import onnx_asr
onnx_asr.load_model("nemo-parakeet-tdt-0.6b-v3", quantization="int8")
print("speech model ready")
PY

if [ -f "$UNIT" ]; then
  sed -i "s|^ExecStart=.*claude_relay.py|ExecStart=$VENV/bin/python $BASE/claude_relay.py|" "$UNIT"
  systemctl --user daemon-reload
  systemctl --user restart hearth-relay.service
  echo "Relay restarted with the new voice engines."
else
  echo "Run relay/install.sh first, then re-run this script."
fi
