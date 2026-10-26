#!/usr/bin/env bash
# CI smoke (B-49): `<command...> --semantic-backend-check [extra...]` starts llama.cpp the way the app does, and a
# CPU backend module of the expected x86 level loads from the expected folder (ggml logs "load_backend: loaded CPU
# backend from <path>" on stderr). Writes <record>/<label>.json (the app's JSON line), <label>.err and
# <label>-backend.json ({"label", "exit", "cpu_backend"}).
#   semantic-backend-smoke.sh <record dir> <label> <folder regex> <level regex> <command...> [++ extra args]
# (`++`, not `--`: SDE takes `--` before the program it runs.)
# Example: semantic-backend-smoke.sh rec deb 'lib/Borealis/ggml/' 'haswell|icelake' /usr/bin/borealis
set -uo pipefail
dir=$1 label=$2 where=$3 level=$4
shift 4
cmd=()
while [ "$#" -gt 0 ] && [ "$1" != ++ ]; do cmd+=("$1"); shift; done
if [ "${1:-}" = ++ ]; then shift; fi
mkdir -p "$dir"
"${cmd[@]}" --semantic-backend-check "$@" > "$dir/$label.json" 2> "$dir/$label.err"
code=$?
cat "$dir/$label.json"
loaded=$(grep -h 'loaded CPU backend from' "$dir/$label.err" | tail -n 1)
echo "$label: ${loaded:-no CPU backend loaded}"
grep -hiE 'SDE-ERROR|illegal instruction|uncaught target signal|not valid for specified chip' "$dir/$label.err" | head -n 3
variant=$(printf '%s' "$loaded" | sed -nE 's/.*ggml-cpu-([a-z0-9_]+)\.(dll|so).*/\1/p')
printf '{"label":"%s","exit":%s,"cpu_backend":"%s"}\n' "$label" "$code" "$variant" > "$dir/$label-backend.json"
fail=0
if [ "$code" -ne 0 ]; then echo "::error::$label: --semantic-backend-check exited $code"; fail=1; fi
if ! printf '%s' "$loaded" | grep -qE "$where"; then
  echo "::error::$label: the CPU backend did not load from a folder matching '$where'"
  fail=1
fi
if ! printf '%s' "$variant" | grep -qxE "$level"; then
  echo "::error::$label: CPU backend level '$variant', expected '$level'"
  fail=1
fi
exit "$fail"
