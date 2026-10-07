#!/usr/bin/env bash
# Brings this computer's background services up to date with the project files. Called by the app's "update" popup.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -f "$HOME/.config/systemd/user/hearth-node.service" ]; then bash "$ROOT/relay/install-node.sh" >/dev/null; fi
PY="$HOME/.local/share/hearth-relay"
if [ -d "$PY" ]; then   # the optional voice engine
  install -m 644 "$ROOT/relay/claude_relay.py" "$PY/claude_relay.py"
  install -m 755 "$ROOT/relay/cm_hook.py" "$PY/cm_hook.py"
  [ -f "$PY/nl_tts_server.py" ] && install -m 755 "$ROOT/relay/nl_tts_server.py" "$PY/nl_tts_server.py"
  systemctl --user try-restart hearth-relay.service hearth-nl-tts.service 2>/dev/null || true
fi
echo ok
