# The Hearth server

The server is where Hearth lives when your laptop is closed. It runs the chats, keeps the shared copy of your projects and chats, and sends you messages.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/install.sh | bash
```

The installer is a text screen with six steps: look around, choose how to install, choose an address, install, sign in to Claude, connect your devices. It works on Debian, Ubuntu and Fedora style systems and needs Node.js 18 or newer (the script installs it if missing).

You can also run it from a copy of this repository: `node server/hearth-server.js`.

### Direct or Docker

| | Directly on the server | Docker |
|---|---|---|
| What it does | Installs Hearth as a service (`hearth`), uses Caddy for HTTPS | One `docker compose` folder in `/opt/hearth` with Hearth and Caddy |
| Good for | Most people | People who already use Docker |
| Where is my data? | `~/projects`, `~/.config/hearth` of the server user | `/opt/hearth/data/…` |
| Sign in to Claude | The installer opens it | `cd /opt/hearth && sudo docker compose run --rm -it hearth claude` |

### Address

- **Free address (recommended):** the installer builds a name from your server's internet address, like `1-2-3-4.sslip.io`, and Caddy gets a real HTTPS certificate for it.
- **Your own domain:** point an A record at the server first.
- **No HTTPS:** only for tests on a trusted network. Traffic is not encrypted.

Ports **80** and **443** must be reachable from the internet. The installer opens them in `ufw` / `firewalld` if one is active, but a firewall at your hosting company is yours to open.

### Options for scripts

```
node server/hearth-server.js --yes --mode direct --address auto --name "My server"
```

| Option | Meaning |
|---|---|
| `--yes` | take the easy choice for every question |
| `--mode direct\|docker` | how to install |
| `--address auto\|none\|your.domain` | which address |
| `--name "text"` | what the server is called in the apps |
| `--dry-run` | show what would happen, change nothing |

## Everyday commands

```
hearth-server code        a new code to connect a device (valid 10 minutes)
hearth-server status      is it running?
hearth-server update      how to update
hearth-server uninstall   remove Hearth (your projects stay)
```

Direct mode: `sudo systemctl status hearth`, `sudo journalctl -u hearth -n 50`.
Docker mode: `cd /opt/hearth && sudo docker compose logs`.

## What the server can and cannot do

- It only works inside the projects folder.
- It never uses "Full auto" (skip all questions) unless you start it with `--allow-bypass`.
- The `hearth` service runs without the right to become root (`NoNewPrivileges`).
- Anyone with the server's secret key can use Claude on your server. Treat the pairing link like a password. To get a new key delete `~/.config/hearth/relay.json` and restart the service; then connect your devices again.

## Updating

Run the install line again. It is safe to run twice: your settings, chats and projects stay.
