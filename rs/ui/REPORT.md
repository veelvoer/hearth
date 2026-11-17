# rs/ui report

## Build (run, passes)
`cd rs && cargo build -p hearth-ui` -> `Finished dev profile` (software renderer; no fontconfig needed).
Cargo features: slint 1.19 `default-features=false`, `std, compat-1-2, renderer-software, software-renderer-systemfonts`. build.rs compiles `src/app.slint`.
Only warnings: Slint says `easing` properties in the Motion global are not exposed to Rust (harmless).
Render: `cargo run -p hearth-ui --bin shots [outdir]` (default ~/hearth-shots/ui) -> 54 PNGs, headless, via a custom Platform + MinimalSoftwareWindow, Motion.scale = 0.
(Mid-session rs/desktop briefly had a dev-dependency `slint/backend-testing` that does not exist; I built through a scratch workspace ~/hearth-shots/ws meanwhile. The real workspace build works now.)

## PNGs I looked at (all in ~/hearth-shots/ui)
phone 360x800 dark: chats, chats-swipe, chat, chat-top, home, history, settings, settings-devices, onboarding, pairing, call, prompt, motion-1-mid.
phone light: home, settings-keys, (chat/chats earlier iterations not re-viewed after last fixes).
wide 1180x780 dark: chat, chat-top, home, settings-devices, settings-appearance. wide light: chat-top, home, settings-appearance, prompt.
watch 400x400: dark home, dark chats, light chats.
NOT viewed individually: phone-light chat/chats/history/call/onboarding/pairing/prompt/updates, wide-*-history, wide-*-onboarding, wide-dark-prompt/updates, wide-light-chat/settings-devices/updates, watch-light-home, phone-dark-settings-updates. They are produced by the same code and may hide small defects.
Reference compared: ~/hearth-shots/ref (m3 dark sessions/chat/home/settings) from desktop/test/shots.js.

## What works (seen)
Tonal M3 surfaces dark+light, pill fields/buttons, 28px cards, floating dock (pill + sliding marker + new-chat square; compact variant at foot of list on wide; status pill on wide), cookie usage rings with arc progress, tool cards, markdown blocks (heading, bullets with bold lead, quote, code), queued message with Remove / Send now, composer with Stop beside Send, "Show earlier messages", swipe row (pin/delete revealed; drag logic in code), wide split layout (list | chat), one-pane phone layout with back arrow, settings nav + pages (devices, appearance, notifications, updates, routines, server, keys, help, about), onboarding, pairing, permission sheet, call, update card, watch home/chats on round display.
Motion: springs are cubic-bezier overshoot easings (Motion.spring). Measured: after a tab change `duration_until_next_timer_update()` is Some (animating); at idle it is None, so nothing animates idle. Mid-animation frame (motion-1-mid.png) shows the marker moving.

## Not tested / limits
- No real pointer input was run (swipe drag, dock marker drag, taps, text input) - only static renders; the swipe and dock-drag code compiles and the swiped state is rendered via the `swiped` model field.
- Frosted dock is only a 95%-opaque tonal layer (software renderer has no blur).
- Scroll is bottom-anchored in chat via a binding; user scrolling not tested. No idle CPU / memory / binary size measurements.
- Fonts: Noto Sans / JetBrains Mono from the system by name (Type.family / Type.mono); bundle fonts for Android/Windows.
- Hangup icon is an X, no phone glyph; cookie is a 90-point polygon.

## Public API (contract for rs/desktop, rs/phone, rs/watch)
Rust: `slint::include_modules!()` exports `App`, `WatchApp`, globals `Palette`, `Motion`, `Type`, structs `ChatRow, Msg, Tile, NavItem, Device, Usage, SettingsState, WatchChat`.
Everything below is on the root `App` (set with `app.set_<name>`, handlers with `app.on_<name>`); nothing needs reaching into children. Inline defaults are demo data.

### Palette (settable from Rust: `app.global::<Palette>().set_<token>()`)
All tokens are in-out colours with a light/dark default binding: bg, surface-low, card, card-high, card-highest, text, muted, faint, outline, outline-soft, accent, on-accent, accent-container, on-accent-container, selected, on-selected, error, ok, scrim, dock; plus `dark` (bool) and `accent-override` (transparent = built-in clay). An Oxide/system theme sets the tokens it has (bg, surface->card, raised->card-high, line->outline-soft, text, muted, accent, ink->on-accent, bad->error) after `set_dark`. `Motion.scale` (0 = reduced motion).
Verified: regress-light-accent-motion-on.png (light + accent override with motion on, theme set after creation) and regress-palette-bg-override.png (bg overridden from Rust).
Bug fixed (rows black in light with accent): colour `animate` on row/pill/switch backgrounds started from the dark default when `dark` was set after creation, so a screenshot taken in the first 120 ms caught the dark colours. Only hover/press state layers animate colour now.

### App properties
```
in-out property <string> screen: "chats";   // chats | chat | home | history | settings | onboarding | pairing | call
in-out property <string> page: "";          // settings page ("" = list on phone, devices on wide)
in property <bool> chat-top: false;
in-out property <bool> prompt: false;
in-out property <bool> update: true;
in property <string> status: "Server online";
in property <string> chat-title: "Fix the login bug on the settings page";
in property <string> chat-sub: "webshop · Sonnet 5.5 · VPS · working now";
in property <bool> chat-working: true;
in-out property <string> draft;
in property <string> filter: "All chats";
in-out property <string> chat-search;
in-out property <string> history-search;
in-out property <string> settings-search;
in property <Usage> usage: {
in property <string> update-version: "1.41.0";
in property <string> update-notes: "Faster chat opening and smoother motion.";
in property <string> prompt-tool: "Edit";
in property <string> prompt-target: "Settings.tsx";
in property <string> prompt-detail: "Claude wants to change 14 lines in src/Settings.tsx.";
in property <string> call-time: "02:14";
in property <string> call-state: "Listening";
in-out property <bool> call-muted: false;
in-out property <string> pair-link;
in property <string> pair-error;
in-out property <int> onboarding-step: 0;
in-out property <SettingsState> settings: {
in-out property <[ChatRow]> chats: [
in-out property <[ChatRow]> history: [
in-out property <[Msg]> messages: [
in-out property <[Tile]> tiles: [
in-out property <[NavItem]> nav: [
in-out property <[Device]> devices: [
```
Screens: `screen` = chats | chat | home | history | settings | onboarding | pairing | call. Wide layout when window width >= 820px. Msg.kind = earlier|user|text|heading|bullet|quote|code|tool|time|meta|queued|working.
SettingsState fields: theme(system|dark|light), motion, frosted, expressive, notify-finished, notify-usage, auto-update, install-on-quit, run-on-server, keep-copy, ask-edit, ask-cmd.
The App updates `settings`, `page`, `screen`, `prompt`, `update` itself on user action and then fires the callback, so the app only has to persist/act.

### App callbacks
```
callback new-chat();
callback open-chat(int);          // index in `chats` (or `history` via open-history)
callback open-history(int);       // index in `history`
callback send(string);            // composer text, already cleared in the UI
callback stop();                  // Stop beside Send
callback composer-option(string); // "edits" | "model" | "effort"
callback send-now(int);           // queued message at index in `messages`
callback remove-queued(int);      // same index
callback show-earlier();
callback pin-chat(int);
callback delete-chat(int);
callback rename-chat(int);        // double click on a row
callback chat-search-edited(string);
callback history-search-edited(string);
callback settings-search-edited(string);
callback filter-clicked();        // project filter pill
callback list-menu();             // dots in the chat list header
callback chat-menu();             // dots / sidebar in the chat header
callback mic();                   // mic in chat header or home (start a call: set screen "call")
callback refresh-usage();
callback usage-range-picked(string); // "7d" | "30d" | "all"
callback tab-changed(int);        // 0 chats, 1 home, 2 history, 3 settings (screen already switched)
callback page-picked(string);     // settings page id (page already set)
callback setting-toggled(string, bool); // id = SettingsState field name; `settings` already updated
callback theme-picked(string);    // "system" | "dark" | "light"
callback device-remove(int);
callback device-add();
callback device-refresh();
callback routine-new();
callback update-install();
callback update-later();
callback prompt-answered(string); // "allow-once" | "allow-always" | "deny" (prompt already closed)
callback pair-connect(string);
callback pair-scan();
callback onboarding-finish();
callback onboarding-skip();
callback call-hangup();
callback call-mute(bool);
```

### WatchApp
props: screen (home|chats), session-pct, session-reset, week-line, chats: [WatchChat{title,sub,working}]; callbacks: open-chat(int), refresh(), open-chats(), back().

### Components (for .slint reuse)
Icon, Tap, IconButton, PillButton, Switch(checked; toggled(bool), no local flip), Field(text; edited(string)), Card, UsageRing, Cookie, VC, Dock, ChatList, ChatRowItem, MessageView, Composer, ChatPane, HomeScreen, HistoryScreen, SettingsScreen, OnboardingScreen, PairingScreen, PermissionPrompt, CallScreen, UpdateCard.
Layout tips: children of non-layout Rectangles are centered unless x/y set; `y` is ignored inside layouts (wrap in `VC`); do not read a layout parent's width in max-width (binding loop).
