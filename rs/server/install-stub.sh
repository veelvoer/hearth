#!/bin/sh
# Hearth server installer stub
# Downloads and runs the hearth-server binary
# Usage: curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/rs/server/install-stub.sh | bash
# (the repo-root install.sh is left alone until the Rust installer is released)

set -e

REPO="${HEARTH_REPO:-veelvoer/hearth}"
BINARY_NAME="hearth-server"
INSTALL_DIR="${HOME}/.local/bin"

say() { printf '\n  \033[1m%s\033[0m\n' "$*"; }
die() { printf '\n  \033[31m✗ %s\033[0m\n\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || die "This installer is for Linux servers."
command -v curl >/dev/null || die "Please install curl first (e.g. sudo apt install curl)."

detect_arch() {
  case "$(uname -m)" in
    x86_64)        echo "x64" ;;
    aarch64|arm64) echo "arm64" ;;
    armv7*)        echo "armv7" ;;
    *)             echo "x64" ;;
  esac
}

ARCH=$(detect_arch)

if command -v $BINARY_NAME >/dev/null 2>&1; then
  exec $BINARY_NAME "$@"
fi

say "Downloading Hearth server binary…"
mkdir -p "$INSTALL_DIR"

GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"
LATEST=$(curl -fsSL "$GITHUB_API" | grep tag_name | head -1 | cut -d'"' -f4) || die "Could not get latest release."

BINARY_URL="https://github.com/${REPO}/releases/download/${LATEST}/hearth-server-linux-${ARCH}"
TMPFILE=$(mktemp)
trap "rm -f '$TMPFILE'" EXIT

curl -fsSL -o "$TMPFILE" "$BINARY_URL" || die "Could not download hearth-server for linux-${ARCH}."

# Verify the checksum: a download that does not match is never installed
CHECKSUM_URL="https://github.com/${REPO}/releases/download/${LATEST}/hearth-server.sha256"
WANT=$(curl -fsSL "$CHECKSUM_URL" | grep " \*\?hearth-server-linux-${ARCH}\$" | awk '{print $1}') || true
[ -n "$WANT" ] || die "This release has no checksum for hearth-server-linux-${ARCH}."
if command -v sha256sum >/dev/null 2>&1; then GOT=$(sha256sum "$TMPFILE" | awk '{print $1}'); else GOT=$(shasum -a 256 "$TMPFILE" | awk '{print $1}'); fi
[ "$GOT" = "$WANT" ] || die "The download did not match its checksum. Nothing was installed."

chmod +x "$TMPFILE"
mv "$TMPFILE" "$INSTALL_DIR/$BINARY_NAME"

say "Starting installer…"
if [ ! -t 0 ] && (: </dev/tty) 2>/dev/null; then
  exec "$INSTALL_DIR/$BINARY_NAME" "$@" </dev/tty
fi
exec "$INSTALL_DIR/$BINARY_NAME" "$@"
