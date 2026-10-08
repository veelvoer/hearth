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
say() { printf '\n  \033[1m%s\033[0m\n' "$*"; }
command -v node >/dev/null || { echo "Node.js is needed (run the Hearth server installer first)."; exit 1; }

say "Hearth support desk"
echo "  Visitors write to you inside the app. You answer by replying to an email."
echo
MAILBOX="${SUPPORT_USER:-}"
if [ -z "$MAILBOX" ]; then read -r -p "  Support mailbox [hearth.support1@gmail.com]: " MAILBOX </dev/tty || true; MAILBOX="${MAILBOX:-hearth.support1@gmail.com}"; fi
SUPPORT_PASS_GIVEN="${SUPPORT_PASS:-}"
if [ -z "${SUPPORT_PASS:-}" ]; then
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
umask 077
cat > /etc/hearth-support.env <<EOF
SUPPORT_USER=$MAILBOX
SUPPORT_PASS=$SUPPORT_PASS
SUPPORT_OWNERS=$OWNERS
SUPPORT_DATA=/var/lib/hearth-support
SUPPORT_PORT=47610
EOF
chmod 600 /etc/hearth-support.env
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

if [ -f /etc/caddy/hearth.caddy ]; then
  mkdir -p /etc/caddy/hearth.d; [ -e /etc/caddy/hearth.d/00-hearth.caddy ] || echo "# extra routes for this server" > /etc/caddy/hearth.d/00-hearth.caddy
  grep -q "hearth.d/" /etc/caddy/hearth.caddy || sed -i '0,/reverse_proxy/s//import \/etc\/caddy\/hearth.d\/*.caddy\n\treverse_proxy/' /etc/caddy/hearth.caddy
fi
if [ -d /etc/caddy/hearth.d ]; then
  cat > /etc/caddy/hearth.d/support.caddy <<'EOF'
handle_path /support/* {
	reverse_proxy 127.0.0.1:47610
}
EOF
  systemctl reload caddy || systemctl restart caddy
else
  echo "  (No Hearth address found. Run the Hearth server installer again, then this script.)"
fi
sleep 2
say "Done"
curl -fsS http://127.0.0.1:47610/health && echo
echo "  Test it: open Hearth → Settings → Support, and write a request."
