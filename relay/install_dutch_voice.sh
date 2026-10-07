#!/usr/bin/env bash
# Installs a much more natural Dutch voice (Coqui XTTS-v2) next to the normal voice engine. Run on the laptop:
#   bash relay/install_dutch_voice.sh
# ~3 GB download, ~3 GB of RAM while it runs, CPU only. The model's license (CPML) allows personal, non-commercial use only.
set -euo pipefail
BASE="$HOME/.local/share/hearth-relay"
VENV="$BASE/venv-nl"
UNIT="$HOME/.config/systemd/user/hearth-nl-tts.service"
HERE="$(cd "$(dirname "$0")" && pwd)"

read -r -p "XTTS-v2 is licensed for non-commercial use (Coqui Public Model License). Continue? [y/N] " a
[ "${a:-n}" = "y" ] || { echo "Cancelled."; exit 1; }

mkdir -p "$BASE" "$(dirname "$UNIT")"
command -v uv >/dev/null || python3 -m pip install --user --quiet uv
UV="$(command -v uv || echo "$HOME/.local/bin/uv")"
[ -d "$VENV" ] || "$UV" venv --python 3.12 "$VENV"      # XTTS needs a Python it supports; uv fetches 3.12 itself
"$UV" pip install --python "$VENV/bin/python" --quiet coqui-tts "transformers<4.50" torch torchaudio --extra-index-url https://download.pytorch.org/whl/cpu
install -m 755 "$HERE/nl_tts_server.py" "$BASE/nl_tts_server.py"

echo "Downloading the voice model (first time only)..."
COQUI_TOS_AGREED=1 "$VENV/bin/python" - <<'PY'
from TTS.api import TTS
t = TTS("tts_models/multilingual/multi-dataset/xtts_v2")
t.tts(text="Hallo, dit is een test.", speaker="Ana Florence", language="nl")
print("Dutch voice ready")
PY

cat > "$UNIT" <<EOF
[Unit]
Description=Hearth Dutch voice (XTTS-v2)

[Service]
Environment=COQUI_TOS_AGREED=1
ExecStart=$VENV/bin/python $BASE/nl_tts_server.py
Restart=always
RestartSec=5
Nice=5

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now hearth-nl-tts.service
systemctl --user restart hearth-relay.service 2>/dev/null || true
echo "Done. Dutch replies are now spoken with the new voice."
