# Porting the relay (and then the desktop app) to Rust

Goal (asked 2026-11-16): no Node anywhere. The Rust relay must behave exactly like `desktop/relay/*.js`;
the 41 test files in `desktop/test` are the spec. Run them against Rust with
`HEARTH_RUST_FRONT=$PWD/desktop/relay-rs/target/release/hearth-relay sh desktop/test/run-all.sh`
(`relay/standalone.js` execs the binary when that variable is set).

## Layout (src/)
- `main.rs`      CLI: full relay (default), `--version`, `--link`, `--core-only`, `mcp-ask` (replaces ask-mcp.js), watchdog parent (`HEARTH_MEM_CHILD`)
- `jsu.rs`       JavaScript-compatible helpers (trim, slice16, String(), toFixed)
- `util.rs`      Value access helpers, random ids, time, files, hashing, gzip
- `cfg.rs`       relay.json read/merge-write
- `core.rs`      chat logs: list/find/summarize/sessions/messages/search/usage/stats/report
- `relay.rs`     the server: state, routes (port of server.js `start()`)
- `jobs.rs`      running Claude (startJob), events, attach (ndjson streams), stop
- `hooks.rs`     permission hooks, pending questions, decisions, SSE events
- `shares.rs`    shared chats (shares.js)
- `chathub.rs`   chat sync hub + agent (chathub.js)
- `filehub.rs`   file sync hub + agent (filehub.js)
- `apikeys.rs`, `perms.rs`, `weekplan.rs`, `safety.rs`, `tools.rs`, `selfupdate.rs`, `pricing.rs`

## Status
- [x] read every JS module
- [x] util/cfg/http plumbing
- [x] core (sessions, report, stats)
- [x] jobs + hooks + events
- [x] all GET routes
- [x] all POST routes
- [x] shares, guests
- [x] chathub/filehub (hub + agent), link/agent loop
- [x] apikeys + /v1
- [x] watchdog, standalone args
- [x] full test suite green against Rust
- [ ] installer/selfupdate ship the binary only
- [x] desktop: Tauri app (desktop/tauri) with the relay inside (lib target of this crate); every `cm.*` call in Rust, pages unchanged (src/shim.js)

2026-11-16: every test file passes against the Rust relay with no Node in the server (HEARTH_RUST_FRONT=<bin> sh desktop/test/run-all.sh).

2026-11-17: desktop/tauri replaces Electron in releases. Build: `cd desktop/tauri && npx @tauri-apps/cli@2 build`.
Tests inside the real window: HEARTH_EVAL=<script.js> HEARTH_TEST_LOG=<file> (the script calls cm.* and logs through `test:log`).
