# rs/xtask and rs/server report

Test run (private 2-member workspace, because `rs/Cargo.toml` currently lists `desktop`, whose manifest does not resolve: `slint` has no feature `backend-testing`; not my area):

    cargo test -p xtask -p hearth-server
    hearth-server  tests/server.rs : test result: ok. 28 passed; 0 failed
    xtask          tests/xtask.rs  : test result: ok. 8 passed; 0 failed
    (lib/bin/doc targets: 0 tests, ok)

`cargo xtask ...` works through the new alias in `rs/.cargo/config.toml` (`xtask = "run -p xtask --"`).

## Ported (source -> Rust)

| Source | Rust | Tested how |
|---|---|---|
| server/hearth-server.js: questions, steps 1-6, banner, wrap, instances (`--instance`, one per person, port + domain clash), `--prefix`, `--dry-run`, `--yes` | `server/src/{cli,install,ui,sysx,manage}.rs` | Installer run on a fake root with scripted answers: direct + own domain, free sslip.io address, "no padlock", docker, second person, clash/refusals, dry run writes nothing, run twice, uninstall (yes/no) |
| systemd unit (runs the Rust `hearth-relay` directly, `%`/`$` escaped, name quoted) | `texts.rs unit_text` | exact string compare |
| Caddy/HTTPS (Caddyfile, `.d` import dir, main Caddyfile import line, caddy.service restart drop-in, old-Caddy warning, ufw/firewalld) | `install.rs setup_caddy/open_firewall`, `texts.rs caddy_text` | Caddyfile exact; files written on fake root; one import line after 2 runs; removed again on uninstall. ufw/firewalld/apt/dnf commands NOT run |
| docker/compose (Dockerfile copies the static relay binary, compose with/without Caddy) | `texts.rs` | content asserts; files on fake root. `docker compose up` NOT run |
| health check (TCP + `GET /health` must be 200) | `detect.rs tcp_up` | mock server: 200 ok, 500 not ok, closed port not ok |
| real public IP over HTTP (ipify, ifconfig.me, icanhazip; IPv4 validated; `--public-ip` / `HEARTH_PUBLIC_IP` override) | `detect.rs` | mock server (bad answer skipped, good one taken) |
| `update`: GitHub latest -> download hearth-relay + hearth-server assets, sha256 from `hearth-relay.sha256` / `hearth-server.sha256` (same as selfupdate.rs), staged `--version` check, `.prev` kept, atomic rename, install.json bumped, restart | `update.rs`, `manage.rs update` | mock GitHub: both replaced; bad checksum refused; program reporting another version refused; old stays; release without server asset still updates relay; bad tag refused; not-newer does nothing |
| code / link (pair code via `POST /pair/new`, `hearth://pair?...` link), status, start/stop/restart, logs, login, uninstall | `manage.rs`, `cli.rs` | link encoding, code formatting, uninstall; the live HTTP pair call and systemctl/journalctl NOT run |
| install-node.sh (laptop user service, `hearth-node` names kept, `--lan`, voice args) | `local.rs install`, `hearth-server local install` | unit text exact; files on fake HOME. systemctl/linger NOT run (`--no-systemctl`, off in tests) |
| migration from claude-meter-node / old hearth-node (config copied unless relay.json exists, old service+dir retired, voice-engine symlinks, stale Node `*.js` removed) | `local.rs migrate`, `hearth-server migrate` | fake HOME incl. nested config, symlink read-through, idempotent second run, existing pairing never overwritten |
| hooks_* of desktop/tauri/src/sys.rs + relay/install_hooks.py | `hooks.rs`, `hearth-server hooks install|remove|status` | fake HOME: foreign hooks and key order kept, idempotent, backup written, old `cm_hook.py` hook recognised, `hearth-server hook` form, unreadable settings.json is never overwritten (sys.rs would have replaced it with `{}`) |
| relay/cm_hook.py | `hooks.rs hook_output/hook_main`, `hearth-server hook` | decision logic unit tested; the live POST to a relay NOT run |
| relay/call_me.py | `hooks.rs call_me`, `hearth-server call` | message/exit-code logic tested; live call NOT run |
| relay/install.sh, install_voice.sh, install_dutch_voice.sh | `voice.rs`, `hearth-server voice install [--service-only] [--dutch]` | service unit + venv choice + ExecStart patch + engine file refresh tested. venv/pip/model downloads/uv NOT run (network, GBs) |
| relay/update-local.sh | `local.rs update_local`, `hearth-server update --local` | fake HOME: relay unit regenerated, voice files refreshed from the copies embedded in the binary |
| install-stub.sh | `rs/server/install-stub.sh` | `sh -n` only. Now REFUSES a download whose sha256 does not match (the earlier stub ignored a mismatch). Repo-root `install.sh` untouched |
| scripts/set-version.js | `xtask/src/version.rs` | node and xtask run on two copies of the version files in ~/hearth-tmp, 6 steps (feature, fix, major, 7.8.9, fix, 0.0.1): all 6 files, release notes and stdout byte-identical; bad specs rejected by both with nothing changed. Differences: rs/Cargo.toml (workspace version) is also set (node leaves it stale and the release check wants it); version.js keeps the node comment text on purpose; usage text says `cargo xtask` |
| scripts/build-mcpkit.js | `xtask/src/mcpkit.rs` (+ `--check`) | byte-identical to node, with a hostile file (control chars, quotes, emoji, U+2028); the repo's own mcpkit-files.js is verified current |
| scripts/lucide-gen.js | `xtask/src/lucide.rs` | byte-identical to node on synthetic SVGs for every tag (path, circle, ellipse, rect with/without rx, line, polyline, polygon, decimals, odd whitespace); the real lucide-map.json loads. Not run on the real lucide-static package (not installed) |
| scripts/google-tasks-token.js | `xtask/src/google.rs` | whole flow against mock Google + mock Hearth server (auth URL, code exchange form, bearer POST to /admin/options, error paths). Real Google NOT contacted |
| scripts/lint-renderer.js | `xtask/src/lint.rs` | script list + generated eslint configs tested. eslint itself (npx, network) NOT run; moot once desktop/renderer is deleted |
| release workflow `check` job | `cargo xtask check-release [--tag vX.Y.Z]` | fixture: missing notes, TODO notes, each of 5 files out of step, wear, versionCode; passes on the real repo (1.40.0) |

## Python that stays (third-party engines; only installed by `hearth-server voice install`)
- `relay/claude_relay.py` (embedded into the binary): the voice sidecar on 127.0.0.1:47602 (STT/TTS HTTP). Its session/hook/stats handlers are superseded by the Rust relay, but I did not strip them out of the Python file.
- `relay/nl_tts_server.py` (embedded): Dutch voice server.
- pip: kokoro-onnx, onnx-asr[cpu,hub], faster-whisper, piper-tts; Dutch: coqui-tts, torch, torchaudio, transformers<4.50, uv.
- Models: Parakeet TDT 0.6B v3 int8, Kokoro 82M, Piper en_US lessac, Piper nl_BE nathalie, Coqui XTTS-v2.
- Claude Code itself is still installed with `npm install -g @anthropic-ai/claude-code` (Node needed only for that; the Docker image keeps node:22-slim for the same reason).

## Not tested / open
- Nothing was run against the real system: no systemctl, sudo, docker, caddy, ufw, apt, useradd, or the live sign-in. Only logic, files and HTTP against mocks.
- The release workflow does not yet publish `hearth-server-linux-x64|arm64` or `hearth-server.sha256` (the stub and `update` expect them; `update` only warns and still updates the relay if they are missing). The workflow needs a job like the relay-front one, plus a `hearth-server.sha256`. I did not edit release.yml.
- `hearth-server update` compares against `hearth-relay --version`; Docker updates run `docker compose up -d --build`.
- `--version` prints the bare version (the new binary is checked this way before replacing).
- Behaviour changes vs the Node installer: no Node check; compose publishes `127.0.0.1:PORT` when Caddy is used so the host can ask for pairing codes without `docker exec node`; `hearth-server login` now exists (it was mentioned in the JS but never implemented); health check hits `/health` (the JS used `/info`).
- Deleted the old `rs/server/tests/commands.rs` (it only asserted on JSON files it wrote itself) and `rs/xtask/tests/set_version.rs`.
