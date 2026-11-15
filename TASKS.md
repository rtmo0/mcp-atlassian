# Project tasks

## Add an on-demand STDIO transport

**Status:** done

### Motivation

The current server supports only Streamable HTTP. For local, single-user Codex
workflows this requires keeping an HTTP process or container running even when
Atlassian tools are not being used. Add a STDIO mode so an MCP client can start
the server as a child process and stop it together with the client session.

### Scope

- Add an `atlassian-mcp stdio` subcommand alongside the existing `serve` HTTP
  mode.
- Reuse `AtlassianMcpServer`, providers, configuration, and all existing tool
  implementations; do not duplicate tool registration or business logic.
- Enable the appropriate `rmcp` STDIO transport feature and serve MCP over
  standard input/output.
- Keep protocol messages exclusively on stdout. Send logs and diagnostics to
  stderr so they cannot corrupt the MCP stream.
- End the server cleanly when stdin closes or the MCP client terminates it.
- Keep `atlassian-mcp serve`, `/mcp`, authentication, health endpoints,
  metrics, Docker, and Kubernetes behavior backward compatible.
- Document local Codex configuration and required environment variables.

### Expected Codex configuration

```toml
[mcp_servers.atlassian]
command = "/absolute/path/to/atlassian-mcp"
args = ["stdio"]
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

`ATLASSIAN_MCP_API_TOKEN` is not required in STDIO mode because there is no
network-facing MCP endpoint. Jira and Confluence authentication remains
unchanged.

### Acceptance criteria

- Codex can launch `atlassian-mcp stdio` without a separately running service
  or container.
- MCP initialization and `tools/list` succeed over STDIO and expose the same
  Jira and Confluence tools as HTTP mode.
- The child process exits after the client closes the STDIO connection.
- No logging or other non-protocol output is written to stdout.
- Existing Streamable HTTP behavior and tests continue to pass.
- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` pass.
- README contains build, configuration, and smoke-test instructions for both
  HTTP and STDIO modes.

