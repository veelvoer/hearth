# hearth-client report

## Done
- `src/stream.rs`: `run(machine, session, body) -> RunEvents` (async Stream of `RunEvent`). POSTs `/sessions/:id/send` (`new` when empty; null extras dropped), reads ndjson line by line, dedupes by `n`, tracks the chat id from the `session` record, and when the connection ends before `end`/`fail` joins `GET /sessions/:id/stream?from=<last n+1>` (sleep min(400ms*attempt, 4s), attempt reset on a successful join, cap 60, 404 gives a synthetic `end`). Non-200 on send gives a `fail` record with the relay's `error` text; 401 gives "Wrong token". Port of `send` in desktop/tauri/src/main.rs and `net::read_records`.
- `events(machine) -> EventFeed`: `GET /events` SSE, `data: {json}` blocks split on blank lines (comments/pings skipped), reconnect 3 s doubling to 60 s (reset after a connected stream).
- `RelayClient::send(id, text, mode, brief, extra: Value)` and `RelayClient::events()`; `RunStream`/`EventStream` removed (signature of `send` changed: no more cwd/kind args, use `extra`).
- reqwest got the `stream` feature. Dropping a stream stops its task.
- `tests/integration_test.rs`: no longer ignored. Real relay in-process (`hearth_relay::boot::embedded`, free port, temp dir under ~/hearth-tmp, HOME and PATH pointed at it, stub `claude` shell script). Covers pair (wrong code refused, right code works), sessions, usage, messages tail, bad token, send new chat (events, rising `n`, session id, deltas, end), resume (`--resume <id>` seen in the stub's log), send to unknown chat (fail), event feed receives `done`. Plus fake-server tests: dropped run rejoined with `from=3` and dedupe, 404 on rejoin ends the run, refused send, SSE reconnect.

## Not done / caveats
- The 60-attempt cap and the backoff timing are not exercised by a test (would take minutes).
- SSE does not send `Last-Event-ID`/`since` (same as the desktop app).
- The stub claude is a shell script, so the test is Unix only.
