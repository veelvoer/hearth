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

## Public API (keep stable)
Rust: `slint::include_modules!()` exports `App`, `WatchApp`, globals `Palette`, `Motion`, `Type`, struct `ChatRow`.
Globals: `Palette.dark` (in-out bool), `Palette.accent-override` (color, transparent = clay), tokens bg, surface-low, card, card-high, card-highest, text, muted, faint, outline, outline-soft, accent, on-accent, accent-container, on-accent-container, selected, on-selected, error, ok, scrim, dock. `Motion.scale` (float, 0 = reduced motion).
`App` (Window, width >= 820px = wide layout): props `screen` (chats|chat|home|history|settings|onboarding|pairing|call), `page` (settings page id, "" = list/devices), `status`, `chat-top`, `prompt`, `update`, models `chats: [ChatRow]`, `history`, `messages: [Msg]`, `tiles: [Tile]`, `nav: [NavItem]`, `devices: [Device]`; callback `new-chat()`.
Structs: ChatRow{header,title,sub,working,pinned,selected,swiped}, Msg{kind(earlier|user|text|heading|bullet|quote|code|tool|time|meta|queued|working),lead,text,time,status}, Tile{label,value,sub}, NavItem{header,id,label,icon}, Device{name,sub,removable}.
Components (importable from .slint): Icon(name,size,color), Tap, IconButton(icon,kind,size), PillButton(text,icon,kind,h), Switch(checked, toggled), Field(placeholder,icon,text), Card, UsageRing(pct,tone), Cookie(tone), Dock(compact,status,current; go(int), new-chat()), ChatList(chats; open/pin/delete(int)), ChatRowItem(item,swipe; open/pin/delete), MessageView, Composer(working,narrow; send/stop), ChatPane(title,sub,messages,working,show-back,narrow,top; back, send-now(int)), HomeScreen, HistoryScreen, SettingsScreen(page; pick(string), back), OnboardingScreen(step; finish, skip), PairingScreen(link,error; connect(string), scan, back), PermissionPrompt(tool,target,detail; deny, allow-once, allow-always), CallScreen(time,state,muted; hangup), UpdateCard(version,notes; install, later), WatchApp(screen: home|chats).
Layout tips learned: children of non-layout Rectangles are centered unless x/y set; `y` is ignored inside layouts (wrap in `VC`); layout children must not read their own parent's width in max-width (binding loops).
