#!/usr/bin/env bash
# Headless launch smoke for a Linux Borealis binary (run inside a container with Xvfb).
#   smoke.sh <borealis-binary> <screenshot-dir> [label] [file-to-open | created]
# Checks: the GUI starts and stays up, the main window maps, `--mcp` answers over stdio.
set -uo pipefail
BIN="$1"; OUT="$2"; LABEL="${3:-linux}"; OPEN_FILE="${4:-}"
SPLASH_WAIT="${SPLASH_WAIT:-1}"; MAIN_WAIT="${MAIN_WAIT:-20}"
mkdir -p "$OUT"
export WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1
export DISPLAY=:99 NO_AT_BRIDGE=1
Xvfb :99 -screen 0 1400x900x24 -nolisten tcp >/tmp/xvfb.log 2>&1 &
sleep 1
fail=0

echo "== mcp"
M='"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}'
REPLY=$(printf '{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{%s}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{%s}}\n' "$M" "$M" \
  | timeout 60 "$BIN" --mcp 2>/tmp/mcp.err)
echo "$REPLY" | head -c 300; echo
tail -n 2 /tmp/mcp.err
TOOLS=$(echo "$REPLY" | grep -o '"name":"[a-z_]*"' | sort -u | wc -l)
echo "mcp tool names seen: $TOOLS"
echo "$REPLY" | grep -q '"id":2' || { echo "FAIL: no tools/list reply"; fail=1; }

echo "== gui"
dbus-run-session -- "$BIN" >/tmp/gui.log 2>&1 &
PID=$!
sleep "$SPLASH_WAIT"
import -window root "$OUT/$LABEL-1-splash.png"
sleep "$MAIN_WAIT"
import -window root "$OUT/$LABEL-2-main.png"
xdotool search --name Borealis 2>/dev/null | while read -r w; do
  echo "window $w: $(xdotool getwindowname "$w") $(xdotool getwindowgeometry "$w" | tr '\n' ' ')"
done
# Drive the UI cheaply: START > NEW CHARACTER tile (coordinates for the 1200x800 default window).
WIN=$(xdotool search --name '^Borealis$' 2>/dev/null | tail -n 1)
if [ -n "$WIN" ]; then
  xdotool windowmove "$WIN" 0 0 && sleep 1
  xdotool mousemove 180 296 click 1 && sleep 4
  import -window root "$OUT/$LABEL-3-new-character.png"
  xdotool mousemove 990 331 click 1 && sleep 0.5 && xdotool type --delay 40 "Linux Smoke"
  xdotool mousemove 904 762 click 1 && sleep 5
  import -window root "$OUT/$LABEL-4-created.png"
fi
if kill -0 "$PID" 2>/dev/null; then echo "gui alive after $((SPLASH_WAIT + MAIN_WAIT))s"; else echo "FAIL: gui exited"; fail=1; fi
grep -iE "error|panic|warn" /tmp/gui.log | head -n 20
CHARS="$HOME/.local/share/app.borealis/characters"
echo "saved characters: $(ls "$CHARS" 2>/dev/null | tr '\n' ' ')"
echo "data dir: $(ls "$HOME/.local/share/app.borealis" 2>/dev/null | tr '\n' ' ')"
[ "${KEEP_RUNNING:-0}" = 1 ] && exit $fail
kill "$PID" 2>/dev/null
wait "$PID" 2>/dev/null

if [ "$OPEN_FILE" = created ]; then  # re-open a copy of the hero created above
  first=$(ls "$CHARS" 2>/dev/null | head -n 1)
  OPEN_FILE=""; [ -n "$first" ] && cp "$CHARS/$first" "$HOME/Opened-$first" && OPEN_FILE="$HOME/Opened-$first"
fi
if [ -n "$OPEN_FILE" ]; then
  echo "== open file argument: $OPEN_FILE"
  dbus-run-session -- "$BIN" "$OPEN_FILE" >/tmp/gui2.log 2>&1 &
  PID=$!
  sleep "$MAIN_WAIT"
  import -window root "$OUT/$LABEL-5-open-file.png"
  kill -0 "$PID" 2>/dev/null && echo "gui alive with file argument" || { echo "FAIL: gui exited"; fail=1; }
  kill "$PID" 2>/dev/null
fi
exit $fail
