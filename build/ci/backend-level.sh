#!/usr/bin/env bash
# CI record (B-49): the x86 level of the llama.cpp CPU backend module a run loaded, read from ggml's stderr line
# "load_backend: loaded CPU backend from <path>/ggml-cpu-<level>.<dll|so>" in <log>. Writes
# <record dir>/<name>-backend.json ({"cpu_backend": "<level>"}) and fails unless the level matches <regex>.
#   backend-level.sh <log> <record dir> <name> <level regex>
set -uo pipefail
log=$1 dir=$2 name=$3 want=$4
level=$(grep -h 'loaded CPU backend from' "$log" | tail -n 1 | sed -nE 's/.*ggml-cpu-([a-z0-9_]+)\.(dll|so).*/\1/p')
mkdir -p "$dir"
printf '{"cpu_backend":"%s"}\n' "$level" > "$dir/$name-backend.json"
echo "$name: CPU backend ${level:-none}"
if ! printf '%s' "$level" | grep -qxE "$want"; then
  echo "::error::$name: CPU backend '$level', expected '$want'"
  exit 1
fi
