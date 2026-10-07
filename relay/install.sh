#!/usr/bin/env bash
# Installs the optional local VOICE ENGINE (Python: Parakeet + Kokoro) as a user service on 127.0.0.1:47602.
# The chat relay itself is Node (install-node.sh) and forwards /voice/* here.
set -euo pipefail
SRC="$(cd "$(dirname "$0")" && pwd)/claude_relay.py"
DEST="$HOME/.local/share/hearth-relay"
UNIT="$HOME/.config/systemd/user/hearth-relay.service"
PYBIN=/usr/bin/python3; [ -x "$DEST/venv/bin/python" ] && PYBIN="$DEST/venv/bin/python"   # voice env, if installed
mkdir -p "$DEST" "$(dirname "$UNIT")"
install -m 755 "$SRC" "$DEST/claude_relay.py"
install -m 755 "$(dirname "$SRC")/cm_hook.py" "$DEST/cm_hook.py"
cat > "$UNIT" <<EOF
[Unit]
Description=Hearth relay (LAN only)
After=network-online.target

[Service]
ExecStart=$PYBIN $DEST/claude_relay.py
Environment=PATH=$HOME/.local/bin:/usr/local/bin:/usr/bin
Environment=CM_PORT=47602
Environment=CM_SIDECAR=1
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable hearth-relay.service
systemctl --user restart hearth-relay.service
loginctl enable-linger "$USER" 2>/dev/null || true   # keep it running when you're logged out
echo
echo "Next, read then run: python3 relay/install_hooks.py  (lets Claude Code tell the relay when it finishes or needs you)"
echo "Pairing token: $(cat "$HOME/.config/hearth-relay/token")"
if command -v firewall-cmd >/dev/null; then
  echo
  echo "Fedora's firewall blocks the relay by default. Allow it on your home network zone with:"
  echo "  sudo firewall-cmd --permanent --add-port=47601/tcp --add-port=47600/udp && sudo firewall-cmd --reload"
fi
