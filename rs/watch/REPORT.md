# rs/watch report

## Ran and saw
- `cargo run -p hearth-pack -- watch`: `rs/target/apk/hearth-watch-rust-debug.apk` (32.8 MB, arm64-v8a + x86_64). apksigner verify: v3 scheme verified. `aapt2 dump badging`: package dev.voer.hearth.rust.watch, uses-feature android.hardware.type.watch, meta-data com.google.android.wearable.standalone=false, minSdk 30.
- `hearth-watch` compiles for aarch64 and x86_64 android (Slint android-activity, software renderer).

## NOT run
- No emulator or watch (no Wear system image, none could be installed); the APK was never installed or started, no screenshots. The Wear data layer client (`hearth_android::wear::ask`) and the HTTP fallback (`bridge::answer_blocking`) have never run on a device.

## What exists
- `watch/ui/watch.slint` first screen (round 400x400 box): usage "session / week %", chat list, tap a chat for last question/answer, "Paste link" from the clipboard to add a server, "via phone"/"direct" indicator. Not yet the rs/ui `WatchApp`.
- Protocol identical to WatchLink.kt/Api.kt: JSON {op,base,id,text} -> {usage|chats|asked/answer|code|error}, request to /hearth/rpc, phone found by capability hearth_phone_link; if no phone is reachable the same handlers run over HTTP.
- Java in the watch APK: HearthNative and HearthActivity only (speech/permission results). No WatchLink on the watch.

## Missing
- WatchApp from rs/ui wired in; send/stop/speech-dictation UI; the Kotlin watch's PhoneLink (DATA_CHANGED server push from the phone) is not ported, so servers come only from pasting a link; tiles/complications if the Kotlin watch has any.

## Update: PhoneLink, voice reply, stop, WatchApp (verified on the host)
- Servers come from the phone: phone `wear::push_machines` (DataClient `/hearth/machines`, JSON [{name,base,token}], like WearSync.kt); watch Java `PhoneLink` (WearableListenerService, 18 lines) -> `HearthNative.machinesChanged` -> `bridge::save_machines` (same filtering as `Servers.save`); at start `wear::fetch_machines` reads what was pushed while closed. The pasted-link button is gone.
- `watch/src/core.rs` (usage, chats, tap = stop if working else dictate+send; `Transport` = phone via `wear::ask` or `Direct` HTTP) and `watch/src/binding.rs` (`hearth_ui::WatchApp` props/callbacks). Voice uses the system recogniser (`recognize_speech`).
- `cargo test -p hearth-watch` (tests/watch_flow.rs, in-process relay, headless): 1 passed (20 s, waits for the seed chat to stop looking busy). Covers: no server message, save_machines (bad JSON ignored, non-http skipped), WatchApp refresh -> chats/week line, tap on a working chat -> "Stopped", tap on an idle chat -> dictation "voice reply" reaches the relay (stub log) and status "Sent", stop op.
- WatchApp has no detail/turn screen or reply/stop buttons: tap does stop-or-reply; the last question/answer ("turn" op exists in the bridge) cannot be shown until rs/ui adds a screen. Not tested on a device: the DataClient and MessageClient JNI calls, PhoneLink, speech.
