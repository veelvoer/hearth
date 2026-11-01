# Hearth design rules

These rules come from the design research we used (platform guides of Apple, Google and Microsoft, WCAG 2.2 AA) and are checked in tests where a program can check them. When you add a screen, go through the checklist at the end.

## Tokens

| Token | Phone (`Theme.kt`) | Desktop (`style.css`) |
| --- | --- | --- |
| Text | `onSurface` 12.5:1 light, 15.5:1 dark | `--ink` |
| Quiet text | `onSurfaceVariant` ≥ 4.5:1 on background and cards | `--muted` ≥ 4.5:1 |
| Accent (text, buttons) | `primary` ≥ 4.5:1, text on it ≥ 4.5:1 | `--clay`, `--onclay` |
| Border of an input or chip | `outlineVariant` ≥ 3:1 | `--line-ctl` ≥ 3:1 |
| Border of a card (decoration) | `outline` (soft) | `--line` |
| Body text | 16sp, small 14sp, label 12sp (all `sp`, so the system font size is respected) | 15px, `.small` 14px, `.label` 12px |
| Spacing | steps of 4/8dp | steps of 4/8px |

`node desktop/test/contrast.test.js` reads the real palettes and fails when a pair drops below its limit. The "System colors" option (Material You on the phone, the desktop accent) picks black or white text by the brightness of the color.

## Rules

1. **Touch targets.** Phone: every tappable thing is at least 48dp tall (chips, text buttons, icon buttons, switch rows). Gaps between targets are at least 8dp. Desktop: at least 32px tall.
2. **Text.** Body 16, small 14, nothing under 12. Quiet text still reaches 4.5:1. No text is cut off at a large system font size: allow wrapping or scrolling.
3. **State is never only a color.** Selected chips also change weight/fill and announce "selected"; working shows a dot **and** the word; errors have words.
4. **Screen readers.** Icon-only buttons have a description. A label and its switch are read together. Chips and text buttons announce as buttons, chips with their selected state. Toasts are read out. Dialogs are real dialogs.
5. **Keyboard (desktop).** Everything works with Tab, Enter, Space and Escape. A visible focus ring (2px accent) on every control. Right-click and hover tips are extras, never the only way.
6. **Hidden gestures are shortcuts.** A long press or a swipe may be a shortcut, but the same action is also reachable by a visible control.
7. **Feedback.** Every action answers at once (a changed button, a toast, a spinner). Errors say what failed, why, and what to do. Nothing destructive happens without a confirmation, and where possible it can be undone (deleted chats and projects go to a trash for 30 days).
8. **Fewer words.** Settings are grouped under headings and fold open; explanations live behind the (i). Words next to an icon are short.
9. **Motion.** Short and purposeful. It is off when the system says reduce motion or the animations setting is off.
10. **Narrow windows.** The desktop window works down to 820px wide: the top bar never overlaps, tabs flow after the logo below 1240px. `desktop/test/layout.js` opens every screen at 820, 1120 and 1500px and reports anything that sticks out.
11. **Offline and errors.** When the server cannot be reached, the screen says so in words, says what happens next (it restarts itself, it tries again) and offers a button.
12. **Privacy.** Ask for a permission where it is needed. Secrets are shown once and never written to logs or files.

## Checklist for a new screen

- [ ] Every control ≥ 48dp (phone) / 32px (desktop), gaps ≥ 8.
- [ ] No text under 12, body 16 / 14.
- [ ] Contrast: new colors added to the palettes are covered by the contrast test.
- [ ] Icon-only buttons have a description; switches have labels.
- [ ] Works with the keyboard and shows the focus ring (desktop).
- [ ] Empty, loading, offline and error states are written.
- [ ] Destructive actions ask first, and can be undone if possible.
- [ ] At a large system font size nothing is cut off; at 820px wide nothing overlaps.
- [ ] Looked at in light and dark, and with "System colors".
- [ ] Release notes written in plain words.
