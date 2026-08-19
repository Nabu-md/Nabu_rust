#!/bin/bash
# Nabu diagnostics capture — launch the app with stderr to a file and tail the
# instrumentation. Read docs/ROOT-CAUSE-AUDIT.md before use.
set -e
APP="/Applications/Nabu.app/Contents/MacOS/app"
LOG="${1:-/tmp/nabu.log}"

pkill -f "Nabu.app/Contents/MacOS/app" 2>/dev/null || true
sleep 1
rm -f "$LOG"
nohup "$APP" > "$LOG" 2>&1 &
echo "launched pid $! -> $LOG"
echo "capturing 8s of boot logs..."
sleep 8
grep -E "\[RESIZE\]|\[WEBVIEW\]|\[WEB\]|\[setup\]|panic|Error" "$LOG" || echo "(no diagnostic lines yet)"
echo
echo "NOW: drag the window edge larger/smaller a few times, then run:"
echo "  grep -E '\[RESIZE\]|\[WEB\]' $LOG"