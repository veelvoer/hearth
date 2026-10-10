#!/usr/bin/env bash
# Sets up the Hearth support desk on the server that runs Hearth.   sudo bash support/install.sh
# It asks for the support mailbox and its app password once, and keeps them in a file only root can read.
set -euo pipefail
[ "$(id -u)" -eq 0 ] || { [ -f "$0" ] && exec sudo -E bash "$0" "$@" || { echo "Run this with sudo: curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/support/install.sh | sudo bash"; exit 1; }; }
HERE="$(cd "$(dirname "$0")" 2>/dev/null && pwd || true)"
if [ ! -f "$HERE/app.js" ]; then   # started with curl | bash: fetch the files first
  TMP="$(mktemp -d)"; curl -fsSL "https://github.com/veelvoer/hearth/archive/${HEARTH_REF:-main}.tar.gz" | tar -xz -C "$TMP" --strip-components=1 || { echo "Could not download Hearth."; exit 1; }
  HERE="$TMP/support"
fi
RUN_USER="${SUDO_USER:-root}"
# which Hearth on this server gets the /support address? the one of this user (or HEARTH_INSTANCE)
INSTANCE="${HEARTH_INSTANCE:-}"
if [ -z "$INSTANCE" ]; then for d in /opt/hearth /opt/hearth-*; do [ -f "$d/install.json" ] && [ "$(node -p "require('$d/install.json').user" 2>/dev/null)" = "$RUN_USER" ] && INSTANCE="$(basename "$d")" && break; done; fi
INSTANCE="${INSTANCE:-hearth}"
say() { printf '\n  \033[1m%s\033[0m\n' "$*"; }
command -v node >/dev/null || { echo "Node.js is needed (run the Hearth server installer first)."; exit 1; }

say "Hearth support desk"
echo "  Visitors write to you inside the app. You answer by replying to an email."
echo
KEEP=""
if [ -f /etc/hearth-support.env ] && [ -z "${SUPPORT_PASS:-}" ]; then
  KEEP=y
  if (: </dev/tty) 2>/dev/null; then read -r -p "  The support mailbox is already set up. Keep it? [Y/n] " KEEP </dev/tty || KEEP=y; fi
  case "$KEEP" in n|N) KEEP="" ;; *) KEEP=y ;; esac
fi
MAILBOX="${SUPPORT_USER:-}"
if [ -z "$MAILBOX" ] && [ -z "$KEEP" ]; then read -r -p "  Support mailbox [hearth.support1@gmail.com]: " MAILBOX </dev/tty || true; MAILBOX="${MAILBOX:-hearth.support1@gmail.com}"; fi
SUPPORT_PASS="${SUPPORT_PASS:-}"
SUPPORT_PASS_GIVEN="$SUPPORT_PASS"
if [ -z "${SUPPORT_PASS:-}" ] && [ -z "$KEEP" ]; then
  echo "  The app password is made in the Google account of that mailbox:"
  echo "  myaccount.google.com → Security → 2-Step Verification (turn on) → App passwords → create one."
  read -r -s -p "  App password (16 letters, nothing shows while you type): " SUPPORT_PASS </dev/tty; echo
fi
SUPPORT_PASS="${SUPPORT_PASS// /}"
OWNERS="${SUPPORT_OWNERS:-}"
[ -n "${SUPPORT_PASS_GIVEN:-}" ] || read -r -p "  Other email addresses that may answer (optional, comma separated): " OWNERS </dev/tty || true

say "Installing…"
mkdir -p /opt/hearth-support /var/lib/hearth-support
cp "$HERE"/app.js "$HERE"/mail.js "$HERE"/server.js "$HERE"/package.json /opt/hearth-support/
(cd /opt/hearth-support && npm install --omit=dev --no-audit --no-fund >/dev/null)
chown -R "$RUN_USER" /var/lib/hearth-support
if [ -z "$KEEP" ]; then
  (umask 077; printf 'SUPPORT_USER=%s\nSUPPORT_PASS=%s\nSUPPORT_OWNERS=%s\nSUPPORT_DATA=/var/lib/hearth-support\nSUPPORT_PORT=47610\n' "$MAILBOX" "$SUPPORT_PASS" "$OWNERS" > /etc/hearth-support.env)
  chmod 600 /etc/hearth-support.env
fi
umask 022
cat > /etc/systemd/system/hearth-support.service <<EOF
[Unit]
Description=Hearth support desk
After=network-online.target
Wants=network-online.target

[Service]
User=$RUN_USER
WorkingDirectory=/opt/hearth-support
EnvironmentFile=/etc/hearth-support.env
ExecStart=$(command -v node) /opt/hearth-support/server.js
Restart=always
RestartSec=5
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=full

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload; systemctl enable --now hearth-support; systemctl restart hearth-support

if [ -f /etc/caddy/$INSTANCE.caddy ]; then
  mkdir -p /etc/caddy/$INSTANCE.d; [ -e /etc/caddy/$INSTANCE.d/00-hearth.caddy ] || echo "# extra routes for this server" > /etc/caddy/$INSTANCE.d/00-hearth.caddy
  grep -q "$INSTANCE.d/" /etc/caddy/$INSTANCE.caddy || sed -i '0,/reverse_proxy/s//import \/etc\/caddy\/$INSTANCE.d\/*.caddy\n\treverse_proxy/' /etc/caddy/$INSTANCE.caddy
fi
if [ -d /etc/caddy/$INSTANCE.d ]; then
  cat > /etc/caddy/$INSTANCE.d/support.caddy <<'EOF'
handle_path /support/* {
	reverse_proxy 127.0.0.1:47610
}
EOF
  chmod 644 /etc/caddy/$INSTANCE.d/*.caddy   # Caddy runs as its own user and must be able to read them
  systemctl restart caddy   # not "reload": Caddy 2.6 (Debian) can crash on reload
else
  echo "  (No Hearth address found. Run the Hearth server installer again, then this script.)"
fi
sleep 2
say "Done"
curl -fsS http://127.0.0.1:47610/health && echo
echo "  Test it: open Hearth → Settings → Support, and write a request."
