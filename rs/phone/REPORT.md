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
- armv7 build, release signing, phone x86_64 APK (only arm64 verified for phone).
