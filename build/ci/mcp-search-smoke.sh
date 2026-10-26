#!/usr/bin/env bash
# CI smoke (B-49): keyword search answers through `<command...> --mcp` (the app binary's MCP mode, keyword-only):
# a search_rules call for a common rules term returns hits. Used on emulated old CPUs (qemu -cpu Conroe, SDE -mrm).
#   mcp-search-smoke.sh <record dir> <label> <command...>
set -uo pipefail
dir=$1 label=$2
shift 2
mkdir -p "$dir"
meta='"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}'
printf '%s\n%s\n' \
  "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server/discover\",\"params\":{$meta}}" \
  "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"search_rules\",\"arguments\":{\"query\":\"grapple\"},$meta}}" \
  | "$@" --mcp > "$dir/$label-mcp.out" 2> "$dir/$label-mcp.err"
code=$?
reply=$(grep '"id":2' "$dir/$label-mcp.out" | head -n 1)
count=$(printf '%s' "$reply" | grep -oE '"count":[0-9]+' | head -n 1 | cut -d: -f2)
echo "$label: mcp exit $code; search_rules 'grapple' hits: ${count:-none}"
grep -hiE 'SDE-ERROR|illegal instruction|uncaught target signal|not valid for specified chip' "$dir/$label-mcp.err" | head -n 3
printf '{"label":"%s","mcp_exit":%s,"keyword_hits":%s}\n' "$label" "$code" "${count:-0}" > "$dir/$label-keyword.json"
if [ "${count:-0}" -lt 1 ]; then
  echo "::error::$label: keyword search gave no hits"
  head -c 600 "$dir/$label-mcp.out"; echo
  tail -n 5 "$dir/$label-mcp.err"
  exit 1
fi
