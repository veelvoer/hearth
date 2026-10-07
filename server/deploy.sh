#!/usr/bin/env bash
# For developers: copies the relay code from this checkout to a server that was set up with the installer, and restarts it.
#   HEARTH_HOST=you@your-server ./server/deploy.sh
set -euo pipefail
HOST="${HEARTH_HOST:?set HEARTH_HOST=user@server first}"
DIR="$(cd "$(dirname "$0")/../desktop/relay" && pwd)"
FILES="core.js server.js standalone.js ask-mcp.js chathub.js filehub.js tools.js buildsig.js"
(cd "$DIR" && tar -c $FILES) | ssh -o BatchMode=yes -o ConnectTimeout=15 "$HOST" '
  set -e
  if [ -f /opt/hearth/install.json ] && grep -q "\"mode\": \"docker\"" /opt/hearth/install.json; then
    sudo tar -C /opt/hearth/relay -x && cd /opt/hearth && sudo docker compose up -d --build >/dev/null && echo "Relay updated (docker)."
  else
    sudo tar -C /opt/hearth/relay -x && sudo systemctl restart hearth && sleep 1.5 && systemctl is-active hearth && echo "Relay updated."
  fi'
