# atlassian-mcp

[![CI](https://github.com/rtmo0/mcp-atlassian/actions/workflows/ci.yml/badge.svg)](https://github.com/rtmo0/mcp-atlassian/actions/workflows/ci.yml)

Read-only [MCP](https://modelcontextprotocol.io) server for **Atlassian Cloud** (Jira and
Confluence), written in Rust on top of the official MCP SDK
[`rmcp`](https://github.com/modelcontextprotocol/rust-sdk).

It ports the read-only toolset of
[`sooperset/mcp-atlassian`](https://github.com/sooperset/mcp-atlassian) 0.21.1
(revision `4e2b836`) with the same tool names, parameters and defaults, and adds
pagination for large Confluence page bodies. It ships as a single static binary.

## Features

- **43 read-only tools**: 29 for Jira and 14 for Confluence.
- **Two transports** from the same binary:
  - `atlassian-mcp stdio`: the MCP client starts the server as a child process. Use this
    for local, single-user setups such as Codex, Claude Code or Cursor.
  - `atlassian-mcp serve`: Streamable HTTP at `/mcp` (stateless, JSON responses), for a
    shared or remote deployment.
- **Jira**: issues, JQL search, projects, fields, agile boards and sprints, users,
  worklogs, transitions, watchers, link types, changelogs, issue dates and SLA,
  development info, Service Desk, ProForma forms, attachments and images.
- **Confluence**: search (plain terms or CQL), pages, children, the space page tree,
  comments, labels, user search, page history, diffs and views, attachments and images.
- **Markdown output**: Jira ADF and Confluence storage XHTML are converted to Markdown.
- **Paginated Confluence pages**: large page bodies come back in chunks, with
  `content.pagination` and `content.next_call`, instead of being silently truncated.
- **Attachments** are returned as MCP `EmbeddedResource` blobs and images as MCP
  `ImageContent`. Size and count limits are configurable.
- **HTTP mode extras**: token auth for `/mcp`, a Host allowlist, health endpoints and
  Prometheus metrics.
- **Read-only by construction**: the code has no write tools and calls no mutating
  Atlassian endpoints.

## Installation

### Prebuilt binaries

Download the archive for your platform from
[Releases](https://github.com/rtmo0/mcp-atlassian/releases). Archives are published for
Linux (x86_64, aarch64) and macOS (Apple Silicon, Intel). Unpack it and put
`atlassian-mcp` somewhere on your `PATH`:

```bash
tar -xzf atlassian-mcp-v0.1.0-aarch64-apple-darwin.tar.gz
install atlassian-mcp-v0.1.0-aarch64-apple-darwin/atlassian-mcp ~/.local/bin/
```

On macOS, a binary downloaded with a browser may be quarantined. Clear the flag with
`xattr -d com.apple.quarantine ~/.local/bin/atlassian-mcp`.

### From source

You need a stable Rust toolchain (edition 2024) plus `cmake` and a C compiler for
`aws-lc-rs`:

```bash
cargo install --locked --git https://github.com/rtmo0/mcp-atlassian
# or, from a checkout:
cargo build --release --locked   # -> target/release/atlassian-mcp
```

### Docker

```bash
docker pull ghcr.io/rtmo0/mcp-atlassian:latest
```

| Tag | Source |
|---|---|
| `latest`, `X.Y.Z`, `X.Y` | Release tags `vX.Y.Z` |
| `edge` | `main` branch |
| `sha-<short>` | Any build |

Images are published for `linux/amd64` and `linux/arm64`. The default command is
`serve`. Pass `stdio` to use the STDIO transport.

## Atlassian credentials

Both modes read Atlassian access from the same environment variables. Create an API
token at <https://id.atlassian.com/manage-profile/security/api-tokens>.

| Variable | Purpose |
|---|---|
| `JIRA_URL` | Jira base URL: `https://<site>.atlassian.net` or `https://api.atlassian.com/ex/jira/<cloud-id>` |
| `JIRA_USERNAME` | Atlassian account email (Basic auth). Leave empty to send the token as a Bearer token |
| `JIRA_API_TOKEN` | API token |
| `CONFLUENCE_URL` | Confluence base URL: `https://<site>.atlassian.net/wiki` or `https://api.atlassian.com/ex/confluence/<cloud-id>` |
| `CONFLUENCE_USERNAME` | Atlassian account email |
| `CONFLUENCE_API_TOKEN` | API token |

Configuring only one product is fine. The other product's tools stay in `tools/list` and
return a clear "not configured" error when called.

## STDIO mode

```bash
atlassian-mcp stdio
```

- The MCP client launches the process and talks JSON-RPC over stdin and stdout.
- stdout carries only MCP messages. All logs go to stderr; set the level with `RUST_LOG`.
- The process exits when the client closes stdin, or on SIGINT or SIGTERM.
- HTTP-only settings are ignored: `ATLASSIAN_MCP_API_TOKEN`, `ATLASSIAN_MCP_REQUIRE_AUTH`,
  ports, listen addresses and the Host allowlist. No network port is opened, including
  the metrics port.

### Codex

`~/.codex/config.toml`:

```toml
[mcp_servers.atlassian]
command = "/absolute/path/to/atlassian-mcp"
args = ["stdio"]
# Forward these variables from the environment Codex runs in:
env_vars = [
  "JIRA_URL",
  "JIRA_USERNAME",
  "JIRA_API_TOKEN",
  "CONFLUENCE_URL",
  "CONFLUENCE_USERNAME",
  "CONFLUENCE_API_TOKEN",
  "JIRA_PROJECTS_FILTER",
  "CONFLUENCE_SPACES_FILTER",
]
```

To store values in the config file instead, use
`env = { JIRA_URL = "https://<site>.atlassian.net", ... }`.

### Claude Code

```bash
claude mcp add atlassian \
  -e JIRA_URL=https://<site>.atlassian.net -e JIRA_USERNAME=you@example.com -e JIRA_API_TOKEN=... \
  -e CONFLUENCE_URL=https://<site>.atlassian.net/wiki -e CONFLUENCE_USERNAME=you@example.com -e CONFLUENCE_API_TOKEN=... \
  -- /absolute/path/to/atlassian-mcp stdio
```

### Other clients (JSON config)

```json
{
  "mcpServers": {
    "atlassian": {
      "command": "/absolute/path/to/atlassian-mcp",
      "args": ["stdio"],
      "env": {
        "JIRA_URL": "https://<site>.atlassian.net",
        "JIRA_USERNAME": "you@example.com",
        "JIRA_API_TOKEN": "...",
        "CONFLUENCE_URL": "https://<site>.atlassian.net/wiki",
        "CONFLUENCE_USERNAME": "you@example.com",
        "CONFLUENCE_API_TOKEN": "..."
      }
    }
  }
}
```

With Docker, run the image with `-i` (not `-t`) and pass the variables through:

```json
{
  "command": "docker",
  "args": ["run", "--rm", "-i", "-e", "JIRA_URL", "-e", "JIRA_USERNAME", "-e", "JIRA_API_TOKEN",
           "ghcr.io/rtmo0/mcp-atlassian:latest", "stdio"]
}
```

### STDIO smoke test

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | atlassian-mcp stdio 2>/dev/null \
  | jq -c 'select(.id == 2) | .result.tools | length'
# -> 43
```

The process exits when `printf` finishes and stdin closes. If stdout contains anything
other than JSON-RPC lines, that is a bug.

## HTTP mode

```bash
ATLASSIAN_MCP_API_TOKEN=devtoken \
JIRA_URL=... JIRA_USERNAME=... JIRA_API_TOKEN=... \
CONFLUENCE_URL=... CONFLUENCE_USERNAME=... CONFLUENCE_API_TOKEN=... \
atlassian-mcp serve        # `serve` is also the default when no subcommand is given
# MCP:     http://127.0.0.1:8080/mcp
# Health:  http://127.0.0.1:8080/healthz, /readyz
# Metrics: http://127.0.0.1:9090/metrics
```

With Docker:

```bash
docker run --rm -p 8080:8080 \
  -e ATLASSIAN_MCP_API_TOKEN=devtoken \
  -e JIRA_URL -e JIRA_USERNAME -e JIRA_API_TOKEN \
  -e CONFLUENCE_URL -e CONFLUENCE_USERNAME -e CONFLUENCE_API_TOKEN \
  ghcr.io/rtmo0/mcp-atlassian:latest
```

The image binds to `0.0.0.0`. The binary itself binds to `127.0.0.1` by default.

Client configuration:

```json
{
  "mcpServers": {
    "atlassian": {
      "type": "streamable-http",
      "url": "http://127.0.0.1:8080/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    }
  }
}
```

### HTTP smoke test

```bash
curl -sS http://127.0.0.1:8080/mcp \
  -H "Authorization: Bearer $ATLASSIAN_MCP_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | jq '.result.tools | length'
```

### Endpoints

| Path | Auth | Purpose |
|---|---|---|
| `/mcp` | Token, if configured | MCP Streamable HTTP |
| `/` | Public | Name, version, products and tool count |
| `/healthz`, `/api/healthz` | Public | Liveness |
| `/readyz`, `/api/readyz` | Public | `503` when no product is configured |
| `:9090/metrics` | Public, separate port | Prometheus metrics |

Clients authenticate with `Authorization: Bearer <token>` or `X-Api-Key: <token>`.

When the server is reachable under a hostname other than localhost, for example behind
a reverse proxy or as a Kubernetes Service, add that hostname to
`ATLASSIAN_MCP_ALLOWED_HOSTS`. Requests with other `Host` headers are rejected as DNS
rebinding protection.

## Configuration reference

### All modes

| Variable | Default | Purpose |
|---|---|---|
| `JIRA_URL`, `JIRA_USERNAME`, `JIRA_API_TOKEN` | — | Jira access (see above) |
| `CONFLUENCE_URL`, `CONFLUENCE_USERNAME`, `CONFLUENCE_API_TOKEN` | — | Confluence access (see above) |
| `JIRA_PROJECTS_FILTER` | — | Default comma-separated project keys for search and project lists. This is a convenience, not a security boundary |
| `CONFLUENCE_SPACES_FILTER` | — | Default comma-separated space keys for search. This is a convenience, not a security boundary |
| `READ_ONLY_MODE` | `true` | Accepted for upstream compatibility. Write tools are not implemented |
| `ATLASSIAN_MCP_REQUEST_TIMEOUT_SECS` | `60` | Atlassian HTTP timeout |
| `ATLASSIAN_MCP_RETRY_MAX` | `2` | Retries for 429, 502, 503 and 504 responses |
| `ATLASSIAN_MCP_MAX_CONCURRENCY` | `4` | Concurrent tool calls per process |
| `ATLASSIAN_MCP_MAX_CONTENT_CHARS` | `100000` | Maximum length of a text or JSON string field. Also caps the Confluence page chunk size |
| `ATLASSIAN_MCP_MAX_ATTACHMENT_BYTES` | `52428800` | Maximum bytes per inline attachment |
| `ATLASSIAN_MCP_MAX_ATTACHMENTS_PER_CALL` | `20` | Maximum attachments per call |
| `ATLASSIAN_MCP_MAX_TOTAL_ATTACHMENT_BYTES` | `104857600` | Maximum total attachment bytes per call |
| `ATLASSIAN_OAUTH_CLOUD_ID` | — | Cloud id, required by the ProForma Forms tools |
| `JIRA_SLA_WORKING_HOURS_ONLY` | `false` | Default for `jira_get_issue_sla.working_hours_only` |
| `JIRA_SLA_WORKING_HOURS_START`, `JIRA_SLA_WORKING_HOURS_END` | `09:00`, `17:00` | Working hours (UTC) for SLA calculations |
| `JIRA_SLA_WORKING_DAYS` | `1,2,3,4,5` | Working days for SLA calculations (1 = Monday) |
| `ATLASSIAN_MCP_BUILD_VERSION` | Crate version | Version reported by the server |
| `RUST_LOG` | `info` | Log filter. Logs always go to stderr |

### HTTP mode only (`serve`)

| Variable | Default | Purpose |
|---|---|---|
| `ATLASSIAN_MCP_LISTEN` | `127.0.0.1` (`0.0.0.0` in Docker) | Listen address |
| `ATLASSIAN_MCP_PORT` | `8080` | HTTP port |
| `ATLASSIAN_MCP_API_TOKEN` | — | Token for `/mcp`. If empty, auth is disabled; use that only for local development |
| `ATLASSIAN_MCP_REQUIRE_AUTH` | `false` | Refuse to start when `ATLASSIAN_MCP_API_TOKEN` is empty |
| `ATLASSIAN_MCP_ALLOWED_HOSTS` | `localhost,127.0.0.1,::1` | Comma-separated `Host` allowlist |
| `ATLASSIAN_MCP_METRICS_PORT` | `9090` | Port for `/metrics`. `0` disables it |
| `ATLASSIAN_MCP_METRICS_LISTEN` | `127.0.0.1` (`0.0.0.0` in Docker) | Metrics listen address |

## Tools

### Jira (29)

| Tool | Purpose |
|---|---|
| `jira_get_issue` | Issue with fields and comments, ADF converted to Markdown |
| `jira_search` | JQL search (`/rest/api/3/search/jql`, `nextPageToken`) |
| `jira_get_project_issues` | Issues of a project |
| `jira_get_all_projects` | Accessible projects |
| `jira_get_project_versions` / `jira_get_project_components` | Project metadata |
| `jira_search_fields` / `jira_get_field_options` | Field discovery and allowed options |
| `jira_get_user_profile` | User profile by email or account id, or `currentUser()` |
| `jira_get_transitions` | Available status transitions |
| `jira_get_worklog` | Worklog entries |
| `jira_get_issue_watchers` | Issue watchers |
| `jira_get_link_types` | Issue link types |
| `jira_get_agile_boards` / `jira_get_board_issues` | Agile boards and board issues |
| `jira_get_sprints_from_board` / `jira_get_sprint_issues` | Sprints and sprint issues |
| `jira_batch_get_changelogs` | Bulk changelogs (Cloud) |
| `jira_get_issue_dates` | Created, updated and resolution dates plus status history |
| `jira_get_issue_sla` | Cycle time, lead time, time in status, due date and more |
| `jira_get_issue_development_info` / `jira_get_issues_development_info` | Development panel (branches, commits, PRs) |
| `jira_get_service_desk_for_project` / `..._queues` / `..._queue_issues` | Service Desk (Server/DC-only upstream) |
| `jira_get_issue_proforma_forms` / `jira_get_proforma_form_details` | ProForma forms (require a cloud id) |
| `jira_download_attachments` / `jira_get_issue_images` | Attachments and inline images |

### Confluence (14)

| Tool | Purpose |
|---|---|
| `confluence_search` | Plain query or CQL search |
| `confluence_get_page` | Page by ID, or by title and space key |
| `confluence_get_page_children` | Child pages and folders |
| `confluence_get_space_page_tree` | Flat page hierarchy of a space |
| `confluence_get_comments` | Page comments |
| `confluence_get_labels` | Content labels |
| `confluence_search_user` | User search via CQL |
| `confluence_get_page_history` | A historical page version |
| `confluence_get_page_diff` | Unified diff between versions |
| `confluence_get_page_views` | View statistics (Cloud) |
| `confluence_get_attachments` | Attachment metadata |
| `confluence_download_attachment` | One attachment as an embedded resource |
| `confluence_download_content_attachments` | All attachments as embedded resources |
| `confluence_get_page_images` | Inline images as MCP image content |

### Large Confluence pages

`confluence_get_page` and `confluence_get_page_history` return page bodies in chunks
instead of truncating them at `ATLASSIAN_MCP_MAX_CONTENT_CHARS`. Pass `content_offset`
(0-based, counted in Unicode characters) and `content_limit` (default
`min(100000, MAX_CONTENT_CHARS)`). Keep requesting chunks while
`content.pagination.has_more` is true, using the arguments in `content.next_call`. With
`include_metadata=true` (the default), the same fields are under `metadata.content`:

```json
{
  "content": {
    "value": "...",
    "format": "markdown",
    "pagination": {
      "offset": 0,
      "limit": 100000,
      "returned": 100000,
      "total": 1288543,
      "has_more": true,
      "next_offset": 100000,
      "page_version": 89
    },
    "next_call": {
      "tool": "confluence_get_page",
      "arguments": {
        "page_id": "6264782851",
        "content_offset": 100000,
        "content_limit": 100000,
        "convert_to_markdown": true,
        "include_metadata": false,
        "expected_version": 89
      }
    }
  }
}
```

Rules for reading chunks:

- For `confluence_get_page`, pass `expected_version` (the `page_version` of the previous
  chunk) whenever `content_offset > 0`. The server returns an error if the page was
  edited between chunks.
- `confluence_get_page_history` is already pinned by its `version` parameter.
- Keep `convert_to_markdown` the same across chunks. Markdown and storage format are
  different strings, so their offsets differ.
- Chunks split on Unicode scalar values and may cut through grapheme clusters or
  Markdown markup. Concatenate all chunks before rendering or parsing.
- Small pages requested without pagination parameters keep the original response shape.

## Security notes

- The code contains only read paths. Two endpoints use POST but only read data: Jira JQL
  search (`/rest/api/3/search/jql`) and the Jira changelog bulk fetch
  (`/rest/api/3/changelog/bulkfetch`).
- `jira_get_issue` accepts `update_history` for schema compatibility but never sends
  `updateHistory=true`, so it does not modify the user's view history.
- Secrets are read only from environment variables. They are never logged or included in
  error messages.
- Attachments are base64-encoded in memory within the configured limits. No temporary
  files are written, so the container can run with a read-only root filesystem.
- `JIRA_PROJECTS_FILTER` and `CONFLUENCE_SPACES_FILTER` are not access controls. Limit
  access through the Atlassian account's permissions.
- An API token may lack scopes for Jira Agile, Confluence user search, dev-status or
  ProForma. Those tools return the upstream-style error and every other tool keeps
  working.

## Development

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

- `tests/stdio.rs` starts the real binary in STDIO mode. It checks that stdout contains
  only JSON-RPC, that `tools/list` returns 43 tools and that the process exits on EOF.
- `src/server/mod.rs` compares every tool's input schema with
  `research/tools-list-0.21.1.json`, the upstream `tools/list` snapshot. Regenerate the
  snapshot with `scripts/snapshot-upstream-tools.sh` (requires
  [`uv`](https://docs.astral.sh/uv/)).

### CI and releases

- **CI** (`.github/workflows/ci.yml`) runs on every push and PR: `fmt`, `clippy` and
  tests on Linux and macOS, plus a Docker build with an in-container STDIO smoke test.
- **Docker** (`.github/workflows/docker.yml`) builds `linux/amd64` and `linux/arm64`
  natively and pushes `ghcr.io/rtmo0/mcp-atlassian` on pushes to `main` and on tags.
- **Release** (`.github/workflows/release.yml`) runs on a `vX.Y.Z` tag. The tag must
  match `version` in `Cargo.toml`. It builds Linux and macOS binaries and attaches them
  with SHA-256 checksums to a GitHub Release.

To cut a release, bump `version` in `Cargo.toml`, commit, then:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

Tool names, parameter contracts and behavior are ported from `mcp-atlassian`
(MIT, Copyright (c) 2024 Hyeonsoo Lee). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
