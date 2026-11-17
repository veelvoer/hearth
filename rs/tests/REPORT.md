# hearth-tests: port of desktop/test/*.test.js

Harness: `src/lib.rs`. Each test starts the real relay as its own process (`hearth-relay-bin`, built from desktop/relay-rs/src/main.rs, `--no-watchdog` except in the watchdog test) with its own HOME and a fake `claude` (Python 3 stubs in PATH, standing in for the node stubs). Scratch dirs are under ~/hearth-tmp. One `#[test]` per JS file keeps the JS case names and assertions; each prints PASS/FAIL lines and `N passed, M failed`.

Full run (real workspace rs/, `cargo test -p hearth-tests --release -- --test-threads=2`, built with LTO off locally via CARGO_PROFILE_RELEASE_LTO=off), every binary: `test result: ok. 1 passed; 0 failed`. Unit tests: `cargo test -p hearth-relay --release --lib`: `test result: ok. 40 passed; 0 failed`.

| JS file | Rust test | Cases | Result |
|---|---|---|---|
| apikeys | tests/apikeys.rs | 26 | ok |
| pair | tests/pair.rs | 10 | ok |
| perms-flow | tests/perms_flow.rs | 30 | ok |
| stop | tests/stop.rs | 7 | ok |
| shares | tests/shares.rs | 60 | ok |
| sync | tests/sync.rs | 27 | ok |
| workflow | tests/workflow.rs | 14 | ok |
| routines | tests/routines.rs | 16 | ok |
| limit | tests/limit.rs | 9 | ok |
| delete-chats | tests/delete_chats.rs | 15 | ok |
| chattags | tests/chattags.rs | 13 | ok |
| computers | tests/computers.rs | 5 | ok |
| rules | tests/rules.rs | 6 | ok |
| checklist | tests/checklist.rs | 16 | ok |
| support-mail | tests/support_mail.rs | 21 | ok |
| weekpins | tests/weekpins.rs | 7 | ok |
| extras | tests/extras.rs | 9 | ok |
| btw | tests/btw.rs | 6 | ok |
| gzip | tests/gzip.rs | 2 | ok |
| rootfiles | tests/rootfiles.rs (calls relay core in-process) | 3 | ok |
| odd-chats | tests/odd_chats.rs | 5 | ok |
| mcp-catalog | tests/mcp_catalog.rs | 7 | ok |
| mcpkit | tests/mcpkit.rs | 8 (2-4 need `node`) | ok (node present here) |
| safety-server | tests/safety_server.rs | 16 | ok |
| watchdog | tests/watchdog.rs | 6 | ok |
| auth-error | tests/auth_error.rs | 2 | ok |
| (smoke) | tests/basic.rs | 5 | ok |
| perms.test.js | `perms::tests` in relay-rs/src/perms.rs | 17 | ok |
| memory.test.js | `util::memory_tests` in util.rs (new `mem_pct_of`/`mem_limit_mb`, used by routes.rs) | 6 | ok |
| safety.test.js | `safety::tests` in safety.rs | 3 tests, all 13 cases | ok |
| weekplan.test.js | `weekplan::tests` in weekplan.rs | 9 | ok |
| selfupdate | `selfupdate::tests` (no JS test existed; version rule only) | 4 | ok |

## Obsolete (Electron/CSS/packaging, nothing to port)
m3css, contrast, globals, layout.js (renderer CSS/JS pages, replaced by Slint UI); installer, packaging (Electron/electron-builder artifacts); updater-flow (Electron updater.js; the Rust desktop has its own updater); silent-exit (Electron main process). Helpers not tests: _wait.js, stub-preload.js, selftest.js, shots.js, run-all.sh.

## Not ported / caveats
- support/test/support.test.js: tests the Node support desk (support/app.js). No Rust support desk exists (rs/server is the installer), so nothing to port; the Node test stays in CI.
- rust-core.test.js, rustcore.test.js (the `--core-only` front) and btw.test.conflict-linuxlarp-*.js (a sync-conflict copy of btw.test.js): not ported or examined.
- Not run on Windows/macOS; tests use Linux tools (sudo stub, git, tar, find, /proc).
- Stubs need python3; mcpkit cases 2-4 need node (skipped with a SKIP line otherwise).
- sync.test.js case numbering follows the JS (no case 24 in apikeys, as in the JS).
- Tests were run with LTO off for speed; CI builds with the workspace's default release profile (not run here).
- Relay bugs found: none. Only relay change: extracted `mem_pct_of`/`mem_limit_mb` from routes.rs inline code into util.rs (same behaviour) so the memory test can be a unit test.
- Not run: the new ci.yml step itself (added to the `rust` job: `cargo test -p hearth-tests --release --manifest-path rs/Cargo.toml -- --test-threads=2`).
