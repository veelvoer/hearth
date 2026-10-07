#!/usr/bin/env bash
# Hearth server installer. Run this on your server:
#   curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/install.sh | bash
# It makes sure Node.js is there, downloads Hearth, and starts the friendly installer.
set -euo pipefail
REPO="${HEARTH_REPO:-veelvoer/hearth}"
REF="${HEARTH_REF:-main}"
DEST="${HEARTH_SRC:-$HOME/.hearth-installer}"

say() { printf '\n  \033[1m%s\033[0m\n' "$*"; }
die() { printf '\n  \033[31m✗ %s\033[0m\n\n' "$*" >&2; exit 1; }
SUDO=""; [ "$(id -u)" -ne 0 ] && SUDO="sudo"

[ "$(uname -s)" = "Linux" ] || die "This installer is for Linux servers."
command -v curl >/dev/null || die "Please install curl first (for example: sudo apt install curl)."
command -v tar >/dev/null || die "Please install tar first."

node_ok() { command -v node >/dev/null && [ "$(node -p 'process.versions.node.split(".")[0]')" -ge 18 ]; }
if ! node_ok; then
  say "Installing Node.js (the engine Hearth runs on)…"
  if command -v apt-get >/dev/null; then
    curl -fsSL https://deb.nodesource.com/setup_22.x | $SUDO -E bash - >/dev/null
    $SUDO apt-get install -y nodejs >/dev/null
  elif command -v dnf >/dev/null; then
    $SUDO dnf install -y nodejs >/dev/null
  else
    die "I can't install Node.js on this system by myself. Install Node.js 18 or newer from nodejs.org, then run this again."
  fi
  node_ok || die "Node.js did not install correctly."
fi

# running from a copy of the repository? then use it, otherwise download the newest version
HERE="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
if [ -n "$HERE" ] && [ -f "$HERE/server/hearth-server.js" ]; then
  SRC="$HERE"
else
  say "Downloading Hearth…"
  rm -rf "$DEST"; mkdir -p "$DEST"
  curl -fsSL "https://github.com/$REPO/archive/$REF.tar.gz" | tar -xz -C "$DEST" --strip-components=1 \
    || die "Could not download Hearth from GitHub. Check the internet connection of this server."
  SRC="$DEST"
fi

# when piped from curl, the keyboard is not stdin: reconnect it so the installer can ask questions
if [ ! -t 0 ] && (: </dev/tty) 2>/dev/null; then exec node "$SRC/server/hearth-server.js" "$@" </dev/tty; fi
exec node "$SRC/server/hearth-server.js" "$@"
