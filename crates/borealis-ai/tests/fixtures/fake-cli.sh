#!/bin/sh
# Fake `claude` / `codex` used by borealis-ai integration tests. Behaviour is chosen by env vars so the
# same script works for either backend:
#   FAKE_MODE   replay | hang | silent | fail | child | big | longline | empty
#   FAKE_STREAM file to replay in `replay` mode
#   FAKE_ARGS_OUT / FAKE_STDIN_OUT / FAKE_ENV_OUT   record argv / stdin / environment for assertions
#   FAKE_SYSFILE_OUT  copy of the --system-prompt-file
#   FAKE_PID_FILE  `child` mode writes the pid of a background grandchild here
if [ "$1" = "--version" ]; then echo "fake-cli 0.0.1"; exit 0; fi
[ -n "$FAKE_ENV_OUT" ] && env > "$FAKE_ENV_OUT"
if [ -n "$FAKE_SYSFILE_OUT" ]; then prev=""; for a in "$@"; do [ "$prev" = "--system-prompt-file" ] && cp "$a" "$FAKE_SYSFILE_OUT"; prev="$a"; done; fi
[ -n "$FAKE_ARGS_OUT" ] && printf '%s\n' "$@" > "$FAKE_ARGS_OUT"
if [ -n "$FAKE_STDIN_OUT" ]; then cat > "$FAKE_STDIN_OUT"; else cat > /dev/null; fi
case "${FAKE_MODE:-replay}" in
  replay) cat "$FAKE_STREAM" ;;
  hang) head -n 1 "$FAKE_STREAM"; sleep 30 ;;
  silent) sleep 30 ;;
  fail) echo "boom at /Users/someone/secret/path" >&2; exit 3 ;;
  child) sleep 60 & echo $! > "$FAKE_PID_FILE"; echo '{"type":"system","subtype":"init"}'; wait ;;
  big) i=0; while [ $i -lt 2000 ]; do echo '{"type":"unknown","pad":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}'; i=$((i+1)); done ;;
  longline) head -c 300000 /dev/zero | tr '\0' a; echo ;;
  empty) exit 0 ;;
  echo_blocks) # needs FAKE_STDIN_OUT: reply with the content block types (and codex -i files) received
    types=$(grep -o '"type":"\(image\|document\|text\)"' "$FAKE_STDIN_OUT" | sed 's/"type":"//;s/"//' | tr '\n' ',')
    imgs=$(printf '%s\n' "$@" | grep -c 'attachment-')
    printf '{"type":"result","subtype":"success","is_error":false,"result":"blocks=%s images=%s"}\n' "$types" "$imgs" ;;
esac
