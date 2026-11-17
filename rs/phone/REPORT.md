# rs/phone report (pipeline + Android glue)

## Ran and saw
- `cargo run -p hearth-pack -- phone --abi arm64-v8a` (PATH=$HOME/.cargo/bin, ANDROID_HOME, ANDROID_NDK_HOME set): built the cdylib with `cargo ndk` (slint 1.19, backend-android-activity-06, renderer-software), generated manifest/resources/Java, aapt2 compile+link, javac+d8, zipalign -P 16, apksigner with ~/.android/debug.keystore. Result `rs/target/apk/hearth-phone-rust-debug.apk` (first run 16.4 MB arm64-v8a; rerun with default ABIs: 33.6 MB, arm64-v8a + x86_64). `aapt2 dump badging`: package dev.voer.hearth.rust, versionName 1.40.0-rust, launchable-activity dev.voer.hearth.rust.HearthActivity, native-code arm64-v8a.
- `cargo check -p hearth-android -p hearth-pack` passes on the host; `hearth-android` has a unit test for the Wear protocol (wear::tests).
- x86_64-linux-android target installed with rustup (works; the watch APK was built for arm64-v8a + x86_64, 32.8 MB).
- hearth-pack needs ANDROID_JAR/JAVA_HOME for slint's own build script (it compiles one Java helper, SlintAndroidJavaHelper.java, inside the Slint crate and embeds the dex; that is Slint's, not ours); the tool sets them.

## NOT run
- No device or emulator: `adb devices` shows one unauthorized physical device; no ~/Android/sdk/emulator and no system-images. Installing them into the SDK was refused by the permission system, so I did not. The APK was never installed or started; no screenshots exist. Nothing about runtime behaviour (Slint fonts on Android, the first screen, JNI calls, WatchLink) is verified.
- The JNI modules (notify, service, intents, clipboard, haptics, wear) only compile (host check + the arm64 build); none was executed.

## What exists
- `phone/pack` bin `hearth-pack` (`phone/pack/src/package.rs`): the Gradle-free packager for phone and watch.
- `phone/glue` crate `hearth-android` (jni 0.21): `notify` (channel+post+cancel+permission), `service` (start/stop call foreground service), `intents` (open_url, share_text, pick_file, take_photo via MediaStore, recognize_speech, speech_available, permissions, read_uri, display_name), `clipboard`, `haptics`, `wear` (protocol types, `ask` client over MessageClient /hearth/rpc, capability hearth_phone_link, `serve`), `bridge` (the op handlers usage/chats/turn/send/stop on hearth-client, used by the phone for the watch and by the watch as HTTP fallback), `results`, `natives` (`export_natives!` macro).
- `phone` crate `hearth-phone`: first screen `ui/phone.slint`: pair by hearth:// link or address+6-digit code, chat list as plain text. It is NOT yet the rs/ui `App` screens.

## Java that remains (generated into the APK by hearth-pack, no Kotlin)
- HearthNative (8 lines): native declarations. HearthActivity (24): extends NativeActivity to forward onActivityResult/onRequestPermissionsResult (file pick, camera, speech, permissions). CallService (25): foreground Service for calls. WatchLink (18): WearableListenerService forwarding /hearth/rpc to Rust.
- Not needed: FileProvider (share is text; the camera writes to MediaStore). The manifest uses a NativeActivity subclass (HearthActivity) with android.app.lib_name.
- Play services client classes come from the Gradle cache AARs (wearable, tasks, basement, base), dexed into the APK.

## Missing
- Wiring `hearth-ui` App to hearth-client: rs/ui `App` only exposes `new-chat()` at the root (open/send/pin/delete/connect callbacks live inside child components, not forwarded), so it cannot be driven from Rust yet; rs/ui must forward them. Not done: chats, streaming chat, home/usage, history, settings, relay-event notifications, call audio, attach flow in the UI.


## Controller (rs/phone/src/controller.rs), verified
- Plain Rust view-model: `State` (screen, chats, history, messages, queued, draft, attachments, usage tiles, settings, call, devices) + `Controller` actions: pair (link or address+code), refresh, search, pin/rename, open_chat, send (queues while working), stop, send_now, remove_queued, set_draft, dictate, attach_file/attach_photo (upload to relay `/upload`, `@path` in the prompt like the Kotlin app), share_last_answer, copy, start_call/hang_up/set_muted, relay `/events` -> notifications, settings persistence, remove_device, load_projects/set_cwd for new chats. Effects go through the `Platform` trait; `platform_android.rs` implements it with hearth-android (compiled for android only).
- `cargo test -p hearth-phone` (tests/controller.rs, in-process relay + stubbed claude): 1 test passed (2.5 s). It covers wrong code/short code errors, pairing and the server list being pushed to the watch, chats load, open existing chat (2 messages), streaming send + queued message auto-run, new chat id from the stream, queue remove, send_now, dictation into the draft, file upload and `@path` in the prompt, share, call start/stop hooks, event -> notification (and suppressed by the setting), pins and settings persisting in a second controller, remove_device back to onboarding.
- Not tested: resume of a dropped run through the controller (client tests cover it), the /events feed end to end inside the controller (on_event was driven directly), search.

## App binding (phone/src/binding.rs, app.rs), verified on the host
- `binding::wire/apply/attach` connect the root `hearth_ui::App` (props + all callbacks) to the controller; `app.rs` (android) = android_main: Slint android-activity, `App`, controller with `AndroidPlatform`, notification permission request, relay event feed, servers pushed to the watch at start. The old `PhoneWindow`/phone.slint is gone.
- `cargo test -p hearth-phone` (needs PKG_CONFIG_SYSROOT_DIR/PKG_CONFIG_LIBDIR from the brief for the host Slint build): 2 tests pass. `tests/app_binding.rs` fires root callbacks headlessly (MinimalSoftwareWindow platform, no event loop; state applied by `apply`) against the in-process relay: onboarding -> pair-connect "addr code" -> chats -> open-chat -> messages -> send + queued + streaming reply -> setting-toggled/theme-picked (Palette.dark flips) -> pin-chat -> permission prompt from a relay event, prompt-answered closes it -> mic/call-hangup -> tab-changed -> device-remove back to onboarding.
- Not wired (no controller action behind them): delete-chat, rename-chat, update-install/later, routine-new, pair-scan (no QR scanner), filter-clicked, show-earlier, usage-range-picked, chat/settings search, most settings toggles besides motion/notifications/theme. Home usage shows pct and reset only (greeting, pace, notes keep the demo defaults). Call screen: foreground service starts, but no audio/speech pipeline.
- Slint on Android with the `software-renderer-systemfonts` feature (pulled by hearth-ui) is unverified on a device; the arm64 build compiles.

## Final APKs (rebuilt after the wiring, verified with aapt2/apksigner, `llvm-nm -D`)
- `rs/target/apk/hearth-phone-rust-debug.apk` 36.3 MB (arm64-v8a + x86_64, classes.dex 0.9 MB), v3 signature verified, package dev.voer.hearth.rust. `libhearth_phone.so` exports `android_main` and the four `Java_dev_voer_hearth_rust_HearthNative_*` symbols. Watch: `hearth-watch-rust-debug.apk` 31.6 MB, package dev.voer.hearth.rust.watch, watch feature present. Neither was installed or run on a device.
- armv7: `rustup target add armv7-linux-androideabi` works, but the Slint Android backend always pulls `skia-safe`, and rust-skia publishes no prebuilt Skia for armv7-linux-androideabi, so the armv7 build fails in `skia-bindings` (seen in the log). Not available without building Skia from source. `--all-abis` exists but will fail until that changes; default ABIs are arm64-v8a + x86_64.
- Note: the Android backend renders with Skia (GL/Vulkan), not the software renderer; `renderer-software` in Cargo.toml only matters on the host (tests).
- Release signing: `hearth-pack <app> --release-sign` reads HEARTH_KEYSTORE, HEARTH_KEYSTORE_PASS (+ optional HEARTH_KEY_ALIAS, HEARTH_KEY_PASS) from the environment, writes `hearth-<app>-rust-release.apk`; without HEARTH_KEYSTORE it stops with a message (seen). A real release signing run was not done (no key used).

## Building on a clean machine / CI (ubuntu-latest)
Needs: Rust stable + `cargo install cargo-ndk`, `rustup target add aarch64-linux-android x86_64-linux-android`, Android SDK with `platforms;android-35` and build-tools (aapt2, zipalign, apksigner, d8; newest installed is used), NDK 27 (`ANDROID_NDK_HOME`, else newest under `$ANDROID_HOME/ndk`), a JDK 17+ with `javac` (`JAVA_HOME`; also used for d8 and for Slint's build script), `curl`, and a debug keystore at `~/.android/debug.keystore` (or `HEARTH_KEYSTORE`; create with `keytool -genkeypair -keystore ~/.android/debug.keystore -storepass android -keypass android -alias androiddebugkey -keyalg RSA -validity 10000 -dname CN=Android Debug`).
```
export ANDROID_HOME=$ANDROID_SDK_ROOT ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018 JAVA_HOME=$JAVA_HOME_17_X64
cd rs && cargo run -p hearth-pack -- phone && cargo run -p hearth-pack -- watch   # -> rs/target/apk/
```
The Play services client AARs are taken from the Gradle cache if present, else downloaded (curl, Google Maven, pinned wearable 19.0.0, tasks 18.3.2, basement 18.9.0, base 18.7.2) into `$HEARTH_TMP/gms` (default ~/hearth-tmp/gms). Verified: with HEARTH_GMS_DIR pointing nowhere the four AARs were downloaded and unpacked; a full clean-runner build was not run (only this machine). Host tests need the PKG_CONFIG_* variables from the brief (fontconfig) here; on ubuntu `libfontconfig1-dev` is enough.

## What the user should expect on first run (untested on any device)
Onboarding -> pairing (paste a `hearth://` link, or "address 123456") -> chats. The app asks for the notification permission at start. Chat streaming, queue, stop, pin, usage numbers, theme/motion/notification settings, permission prompts from the relay, call button (starts the foreground service only), share (chat menu), dictation/attach/photo plumbing exist in the controller but the App has no visible buttons wired for attach/photo/dictation except what rs/ui exposes (`mic` starts a call). Expect rough edges: the first real run is also the first time Skia/fonts/JNI run on Android. Debug app ids never touch the existing Kotlin apps.
