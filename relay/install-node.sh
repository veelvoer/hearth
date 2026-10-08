#!/usr/bin/env bash
# Installs the Hearth chat relay (Node) as a user service on this computer: starts at login, restarts if it dies.
# It serves the desktop app and the phone app on your local network, and forwards voice to the optional voice engine.
set -euo pipefail
SRC="$(cd "$(dirname "$0")/../desktop/relay" && pwd)"
DEST="$HOME/.local/share/hearth-node"
UNIT="$HOME/.config/systemd/user/hearth-node.service"
CFG="$HOME/.config/hearth-node"
mkdir -p "$DEST" "$CFG" "$(dirname "$UNIT")"
# early testers had these under the old names: keep their pairing, then retire the old service
OLD="$HOME/.config/claude-meter-node"
if [ -d "$OLD" ] && [ ! -e "$CFG/relay.json" ]; then cp -a "$OLD/." "$CFG/"; fi
if [ -f "$HOME/.config/systemd/user/claude-meter-node.service" ]; then
  systemctl --user disable --now claude-meter-node.service 2>/dev/null || true
  rm -f "$HOME/.config/systemd/user/claude-meter-node.service"; rm -rf "$HOME/.local/share/claude-meter-node"
fi
link_old() { [ -e "$1" ] && [ ! -e "$2" ] && ln -s "$1" "$2" || true; }   # the optional voice engine keeps working
link_old "$HOME/.local/share/claude-meter-relay" "$HOME/.local/share/hearth-relay"
link_old "$HOME/.config/claude-meter-relay" "$HOME/.config/hearth-relay"
rm -f "$DEST/chatsync.js"
install -m 644 $(ls "$SRC"/*.js | grep -v "/embedded.js$") "$DEST/"
SEED=""; VOICE=""
[ -f "$HOME/.config/hearth-relay/token" ] && SEED="--seed-token-file $HOME/.config/hearth-relay/token" && VOICE="--voice-port 47602 --voice-token-file $HOME/.config/hearth-relay/token"
cat > "$UNIT" <<EOF
[Unit]
Description=Hearth chat relay (local network)
After=network-online.target

[Service]
ExecStart=$(command -v node) $DEST/standalone.js --lan --host 0.0.0.0 --port 47601 --dir $CFG --move-root $HOME/projects $SEED $VOICE
Environment=PATH=$HOME/.local/bin:/usr/local/bin:/usr/bin
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable hearth-node.service
systemctl --user restart hearth-node.service
loginctl enable-linger "$USER" 2>/dev/null || true
sleep 1.5
systemctl --user is-active hearth-node.service
