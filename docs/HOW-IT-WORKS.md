# How Hearth works

```
  phone app ───────────┐
                       ▼
                 SERVER (always on)  ◄──────►  LAPTOP (your projects folder)
          Hearth service + HTTPS (Caddy)         Hearth app + Hearth service
                       ▲
  desktop app ─────────┘
```

- **The server is the hub.** The apps talk to it over HTTPS with a secret key. It runs Claude Code chats, keeps a queue of work and sends notifications.
- **Your laptop links out** to the server (no ports opened on your home network). While the laptop is online the server can hand it work (setting *Run on: Auto / Laptop / Server only*).
- **Chats**: Claude Code stores chats as files. Hearth copies them between computers. Paths are rewritten on the way (`/home/you/projects/x` ⇄ `/home/server/projects/x`), so a chat started on your phone continues on your laptop.
- **Files**: each computer remembers what it last synced with the server. That way Hearth can tell "changed here", "changed there" and "deleted" apart:
  - changed on one side only → that side wins
  - changed on both → the newest wins, the other copy is saved as `name.conflict-<computer>-<time>`
  - deleted → moved to `.hearth-trash` (on every computer); a sync that would delete more than 15 files and more than a quarter of everything it keeps is refused
- **New projects**: a project folder that appears on the server while your computer is away is not copied silently. Hearth asks first (*Install / Later*).
- **Usage**: the 5-hour and weekly limits are read from what the real Claude Code reports. Hearth never touches your Claude login.

## Code map

| Folder | What |
|---|---|
| `desktop/` | Electron app (`main.js`, `renderer/`), tests in `desktop/test/` |
| `desktop/relay/` | The Hearth service (Node.js): `server.js`, `chathub.js` (chat sync), `filehub.js` (file sync), `tools.js` (git, notes, plugins, connections), `core.js` |
| `app/` | Android app (Kotlin, Jetpack Compose) |
| `server/` | Server installer (`hearth-server.js`) and `deploy.sh` for developers |
| `relay/` | Linux helper scripts: the laptop background service and the optional voice engine |
| `branding/` | Logo |
