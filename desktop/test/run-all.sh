#!/bin/sh
# Runs every test file. HEARTH_RUST_FRONT=/path/to/hearth-relay runs the relay tests through the Rust front. Usage: sh desktop/test/run-all.sh
cd "$(dirname "$0")/.." || exit 1
bad=0
# a test that fails can leave its relay running; the next test would then talk to the wrong one
reap() { ps -eo pid,args | awk '/\.cache\/hearth-/ && !/awk/ && !/hearth-node/ {print $1}' | xargs -r kill 2>/dev/null; sleep 0.3; }
for f in test/*.test.js; do
  reap; out=$(timeout 240 node "$f" 2>&1); code=$?; reap
  if [ $code -ne 0 ]; then out=$(timeout 240 node "$f" 2>&1); code=$?; reap; fi   # tests that wait on clocks can miss once on a busy machine: a second miss is a real failure
  last=$(printf '%s\n' "$out" | grep -E "passed|skipped" | tail -1)
  if [ $code -ne 0 ]; then bad=$((bad+1)); echo "FAIL $f  $last"; printf '%s\n' "$out" | grep "^FAIL" | head -3; else echo "ok   $f  $last"; fi
done
echo "failed files: $bad"; [ $bad -eq 0 ]
