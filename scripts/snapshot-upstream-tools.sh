#!/usr/bin/env bash
# Regenerate research/tools-list-0.21.1.json: the read-only `tools/list` output of
# upstream sooperset/mcp-atlassian 0.21.1, used by the schema parity test in
# src/server/mod.rs.
#
# PYTHONHASHSEED is pinned because upstream joins a Python set for the Jira
# `fields` default; the parity test compares that default order-insensitively.
#
# Requires `uv` (https://docs.astral.sh/uv/). Atlassian credentials are fake:
# upstream builds its tool list without contacting Atlassian.
set -euo pipefail

VERSION="${UPSTREAM_VERSION:-0.21.1}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${ROOT}/research/tools-list-${VERSION}.json"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"snapshot","version":"0"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
  # Keep stdin open until the response arrives.
  sleep "${SNAPSHOT_WAIT_SECS:-15}"
} | (cd "$TMP" && env -i PATH="$PATH" HOME="$HOME" \
      READ_ONLY_MODE=true TOOLSETS=all PYTHONHASHSEED=0 \
      JIRA_URL=https://example.atlassian.net JIRA_USERNAME=user@example.com JIRA_API_TOKEN=fake \
      CONFLUENCE_URL=https://example.atlassian.net/wiki CONFLUENCE_USERNAME=user@example.com CONFLUENCE_API_TOKEN=fake \
      uvx --from "mcp-atlassian==${VERSION}" mcp-atlassian --transport stdio) \
  > "$TMP/out.jsonl" 2> "$TMP/err.log" || true

mkdir -p "$(dirname "$OUT")"
if ! python3 - "$TMP/out.jsonl" "$OUT" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
for line in open(src):
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("id") == 2 and "result" in msg:
        tools = sorted(msg["result"]["tools"], key=lambda t: t["name"])
        with open(dst, "w") as f:
            json.dump({"tools": tools}, f, indent=2, ensure_ascii=False)
            f.write("\n")
        print(f"wrote {len(tools)} tools to {dst}")
        break
else:
    sys.exit("no tools/list response from upstream")
PY
then
  echo "--- upstream stderr ---" >&2
  cat "$TMP/err.log" >&2
  exit 1
fi
