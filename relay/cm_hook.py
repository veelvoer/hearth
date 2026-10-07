#!/usr/bin/env python3
"""Claude Code hook -> Hearth relay. Reads the hook JSON on stdin; for permission
requests it can answer allow/deny when you tap the button on your phone."""
import json
import os
import sys
import urllib.request


def main() -> None:
    raw = sys.stdin.read()
    try:
        req = urllib.request.Request(f"http://127.0.0.1:{os.environ.get('CM_PORT', '47601')}/hook",
                                     data=raw.encode(), headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=40) as r:
            out = json.load(r)
        name = json.loads(raw).get("hook_event_name")
    except Exception:
        return  # relay not running: stay out of the way
    if "hookSpecificOutput" in out:  # the Node relay answers with the finished hook output
        print(json.dumps(out))
        return
    decision = out.get("decision")
    if name == "PermissionRequest" and decision in ("allow", "deny"):
        d = {"behavior": decision}
        if decision == "deny":
            d["message"] = "Denied from your phone"
        print(json.dumps({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": d}}))


if __name__ == "__main__":
    main()
