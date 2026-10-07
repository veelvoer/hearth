#!/usr/bin/env python3
"""Adds the Hearth hooks to ~/.claude/settings.json (backs it up first, safe to re-run).

Run this yourself, after reading it. It only adds three hooks that call cm_hook.py:
  Stop               -> tells the relay a turn finished
  PreToolUse         -> (AskUserQuestion only) tells the relay Claude has a multiple-choice question
  PermissionRequest  -> lets the phone Allow/Deny; with no answer the normal prompt appears
The relay ignores all of them unless "Call me" is switched on in the app.
"""
import json
import shutil
from pathlib import Path
from typing import Any

SETTINGS = Path.home() / ".claude" / "settings.json"
CMD = f"python3 {Path.home()}/.local/share/hearth-relay/cm_hook.py"
HOOKS: list[tuple[str, str, int]] = [("Stop", "", 10), ("PreToolUse", "AskUserQuestion", 10), ("PermissionRequest", "", 45)]


def main() -> None:
    data: dict[str, Any] = json.loads(SETTINGS.read_text()) if SETTINGS.exists() else {}
    if SETTINGS.exists():
        shutil.copy(SETTINGS, SETTINGS.with_suffix(".json.bak-hearth"))
    hooks = data.setdefault("hooks", {})
    for event, matcher, timeout in HOOKS:
        groups = hooks.setdefault(event, [])
        if any(h.get("command") == CMD for g in groups for h in g.get("hooks", [])):
            continue
        groups.append({"matcher": matcher, "hooks": [{"type": "command", "command": CMD, "timeout": timeout}]})
    SETTINGS.write_text(json.dumps(data, indent=2) + "\n")
    print(f"Hooks installed in {SETTINGS} (backup: settings.json.bak-hearth)")


if __name__ == "__main__":
    main()
