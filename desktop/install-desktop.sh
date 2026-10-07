#!/usr/bin/env bash
# Adds Hearth to your application menu (and, with --autostart, starts it at login).
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
ENTRY="[Desktop Entry]
Type=Application
Name=Hearth
Comment=Claude usage, Claude Code sessions, voice and calls
Exec=$DIR/node_modules/.bin/electron $DIR
Icon=$DIR/build/icon.png
Terminal=false
Categories=Development;Utility;
StartupWMClass=Hearth"
mkdir -p "$HOME/.local/share/applications"
echo "$ENTRY" > "$HOME/.local/share/applications/hearth.desktop"
if [ "${1:-}" = "--autostart" ]; then
  mkdir -p "$HOME/.config/autostart"
  echo "$ENTRY" > "$HOME/.config/autostart/hearth.desktop"
  echo "Will start at login."
fi
echo "Installed. Find 'Hearth' in your app menu, or run: cd $DIR && npm start"
