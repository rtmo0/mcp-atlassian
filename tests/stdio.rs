//! End-to-end checks for `atlassian-mcp stdio`: the binary is spawned as an MCP
//! client would spawn it, and stdout must contain nothing but JSON-RPC messages.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(30);

fn run_session(input: &str) -> (std::process::ExitStatus, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlassian-mcp"))
        .arg("stdio")
        .env_clear()
        .env("RUST_LOG", "debug")
        // HTTP-only settings must not affect stdio mode.
        .env("ATLASSIAN_MCP_REQUIRE_AUTH", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn atlassian-mcp stdio");

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    // Closing stdin ends the session.
    drop(stdin);

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = rx
        .recv_timeout(TIMEOUT)
        .expect("atlassian-mcp stdio did not exit after stdin was closed")
        .expect("wait for atlassian-mcp");
    (
        output.status,
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn stdio_initializes_lists_tools_and_exits_on_eof() {
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"jira_get_link_types","arguments":{}}}"#,
    ]
    .join("\n")
        + "\n";

    let (status, stdout, stderr) = run_session(&input);
    assert!(status.success(), "exit status {status}, stderr:\n{stderr}");
    assert!(!stderr.is_empty(), "logs are expected on stderr");

    let messages: Vec<Value> = stdout
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("non JSON-RPC line on stdout ({error}): {line:?}"))
        })
        .collect();
    for message in &messages {
        assert_eq!(message["jsonrpc"], "2.0", "unexpected message: {message}");
    }
    let by_id = |id: u64| {
        messages
            .iter()
            .find(|message| message["id"] == id)
            .unwrap_or_else(|| panic!("no response with id {id}; stdout:\n{stdout}"))
    };

    let init = by_id(1);
    assert_eq!(init["result"]["serverInfo"]["name"], "Atlassian MCP");

    let tools = by_id(2)["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 43);
    for name in [
        "jira_get_issue",
        "jira_search",
        "confluence_get_page",
        "confluence_search",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "{name} missing from tools/list"
        );
    }

    // Jira is not configured, so the call returns a tool error, not a protocol error.
    let call = &by_id(3)["result"];
    assert_eq!(call["isError"], true);
    assert!(
        call["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Jira is not configured")
    );
}

#[test]
fn stdio_writes_nothing_to_stdout_when_stdin_closes_before_initialize() {
    let (status, stdout, stderr) = run_session("");
    // No initialize handshake happened; the process must still terminate without
    // writing anything to stdout.
    assert!(stdout.is_empty(), "unexpected stdout: {stdout:?}");
    let _ = (status, stderr);
}
