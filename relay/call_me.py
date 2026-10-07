#!/usr/bin/env python3
"""Rings your phone (and desktop, if enabled) through the Hearth relay.
Usage: python3 call_me.py "Finished the refactor, want to talk it through?"
"""
import json
import os
import sys
import urllib.request


def main() -> int:
    text = " ".join(sys.argv[1:]) or "Claude is calling"
    body = json.dumps({"text": text, "cwd": os.getcwd()}).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{os.environ.get('CM_PORT', '47601')}/call", data=body,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            n = json.load(r).get("listeners", 0)
    except Exception as e:
        print(f"Couldn't reach the relay ({e}). Is it running? Try: systemctl --user restart hearth-relay")
        return 1
    if n == 0:
        print("Relay is up but no phone or desktop is listening. Turn on Call me in the app.")
        return 2
    print(f"Calling ({n} device{'s' if n != 1 else ''} listening).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
