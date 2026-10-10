#!/usr/bin/env bash
# For developers: copies the relay code from this checkout to a server that was set up with the installer, and restarts it.
#   HEARTH_HOST=you@your-server ./server/deploy.sh          (HEARTH_INSTANCE=hearth-name if the server holds several Hearths)
set -euo pipefail
HOST="${HEARTH_HOST:?set HEARTH_HOST=user@server first}"
INST="${HEARTH_INSTANCE:-hearth}"
DIR="$(cd "$(dirname "$0")/../desktop/relay" && pwd)"
FILES="$(cd "$DIR" && ls *.js | grep -v '^embedded.js$' | tr '\n' ' ')"
REMOTE='
  set -e
  if [ -f /opt/$INST/install.json ] && grep -q "\"mode\": \"docker\"" /opt/$INST/install.json; then
    sudo tar -C /opt/$INST/relay -x && cd /opt/$INST && sudo docker compose up -d --build >/dev/null && echo "Relay updated (docker)."
  else
    sudo tar -C /opt/$INST/relay -x && sudo systemctl restart $INST && sleep 1.5 && systemctl is-active $INST && echo "Relay updated."
  fi'
(cd "$DIR" && tar -c $FILES) | ssh -o BatchMode=yes -o ConnectTimeout=15 "$HOST" "INST=$INST; $REMOTE"
