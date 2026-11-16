# Hearth, all in Rust

Asked 2026-11-16: every app (desktop, phone, watch) and the relay in Rust, no Node, no Kotlin, no HTML/JS UI, as light and fast as possible.
Decisions: UI toolkit = **Slint** (native, GPU/software renderer, Android + Wear OS via android-activity). The old apps keep shipping until the Rust one of the same kind is built, tested and the user says to switch (nobody can test on a device overnight: say plainly what was and was not run).

## Layout (workspace `rs/`, its own Cargo.lock; relay stays at `desktop/relay-rs`, used as a path dependency)
- `rs/client`   crate `hearth-client`: everything the UIs share that is not drawing. Relay API client (reqwest+rustls, async tokio), models (Sess, Msg, Machine, Usage...), persistence (settings, machines, pins, names), pairing links, streaming of a run (ndjson with resume), usage thresholds, formatting (ago, clock, markdown to blocks). No UI types. Pure Rust, unit-tested.
- `rs/ui`       crate `hearth-ui`: the Slint design system, `.slint` only plus tiny Rust glue: theme tokens (dark first, light works, M3 Expressive: springs via Slint `animate` with custom easing, cookie/flower shapes as paths, floating dock, elastic press, swipe rows, frosted dock option), and the screens shared by desktop and phone (chats list, chat, home/usage, history, settings pages). Responsive: one screen file works at 360 px and at 1180 px.
- `rs/desktop`  bin `hearth` (Linux, Windows): window, tray, notifications, autostart, system colors (accent/dark, Oxide `~/.config/oxide/theme.json`), single instance, embeds `hearth-relay` in-process, updater. Replaces `desktop/tauri` + `desktop/renderer` + `desktop/main.js`.
- `rs/phone`    cdylib for Android (`aarch64-linux-android`, `armv7`, `x86_64` for the emulator), Slint android-activity backend. Needs JNI glue only for: notifications, foreground service (calls), share/camera/file pick, speech, Wear data layer. Keep Java/Kotlin at zero: use `android-activity` + `jni` crate; where Android forces a Java class (a Service, a WearableListenerService), generate the smallest possible `.java`/manifest in the build and say so; target: no `.kt`.
- `rs/watch`    cdylib for Wear OS, round layout, talks to the phone over Bluetooth (Wear data layer MessageClient request `/hearth/rpc`, same protocol as the Kotlin `WatchLink`/`Api.kt` today), falls back to HTTP.
- `rs/xtask`    bin: replaces scripts/*.js (set-version, packaging, release checks) and `server/hearth-server.js` + `relay/*.sh` (installer, systemd units, updates) as `hearth-server` subcommands.
- `rs/tests`    the 41 JS tests in `desktop/test` rewritten as Rust integration tests (they are the spec of the relay's behaviour; most already run against the Rust relay).

## Rules for everyone
- Match what the existing app does; read the Kotlin / JS first. Do not lose features. Where a feature cannot be ported, list it in `RUSTPLAN.md` under "Not ported".
- One design: tokens in `rs/ui/theme.slint` (background, card, text, muted, accent, on-accent, error; never hardcoded colours elsewhere). Motion springs: fast (0.6/800), move (0.8/380), slow (0.8/200); opacity/colour never overshoot; "Motion" setting turns them off; respect reduced motion.
- Slint software renderer or femtovg; measure idle CPU (must be ~0), memory, binary size, startup. Put the numbers in the report.
- No `unwrap` on I/O, no network call on the UI thread, nothing animates while idle.
- Build with `cargo build --release` in `rs/`; Android with `cargo ndk` (NDK 27 at ~/Android/sdk/ndk, target installed: aarch64-linux-android). Tauri needs the sysroot env from memory only for the old app.
- Do not edit files outside your area without need; never touch `*.conflict-*` files; never run `pkill -f` (kill by pid); never write to /tmp/ox; do not restart the `hearth-node` service (this chat may run under it).
- Commit nothing; the lead commits. Leave `rs/<area>/REPORT.md` with: what works (tested how), what is untested, what is missing.

## Status (2026-11-17, written when the first push was made)
Verified by running (cargo, this laptop): `hearth-client` 29 unit + 5 integration tests pass against the real relay (also builds for aarch64-android); `xtask` and `hearth-server` report 8 + 28 tests (run by their agent, only against fakes/mocks: no real systemd/docker/caddy/github); relay-rs 40 unit tests pass (the old JS test suite is still the main spec: `desktop/test`).
Not finished: `rs/ui` (Slint screens compile, not yet reviewed against the Material 3 Expressive look), `rs/desktop` (platform layer + controller written, its end-to-end test currently FAILS on support mails), `rs/phone` / `rs/watch` (Android packaging and JNI glue started, never run on a device or emulator), `rs/tests` (only part of the JS tests ported). The Kotlin apps, the Tauri desktop app and the Node tooling are still the ones that ship; nothing in `rs/` is released or used by CI yet (the workspace is outside CI).
Not ported: see each `rs/*/REPORT.md` where present.
