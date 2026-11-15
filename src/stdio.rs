use std::sync::Arc;

use anyhow::Context;
use rmcp::ServiceExt;
use rmcp::service::QuitReason;

use crate::config::{Config, build_version};
use crate::metrics::Metrics;
use crate::providers::Providers;
use crate::server::AtlassianMcpServer;
use crate::signal::shutdown_signal;

/// Serve MCP over stdin/stdout for a single client that launched this process.
///
/// stdout carries only MCP protocol messages; all logs go to stderr (see `main`).
/// The server exits when stdin reaches EOF or on SIGINT/SIGTERM.
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let providers = Providers::from_config(&cfg)?;
    // Metrics are recorded in memory only: no listener in STDIO mode, so several
    // client-spawned processes never compete for a port.
    let metrics = Arc::new(Metrics::new(
        &build_version(),
        providers.jira.is_some(),
        providers.confluence.is_some(),
    )?);
    let server = AtlassianMcpServer::new(providers, metrics);
    tracing::info!(
        tools = server.tool_count(),
        "atlassian-mcp serving MCP over stdio"
    );

    let running = tokio::select! {
        running = server.serve(rmcp::transport::stdio_pipes()) => {
            running.context("MCP initialization over stdio failed")?
        }
        () = shutdown_signal() => return Ok(()),
    };

    let cancel = running.cancellation_token();
    let waiting = running.waiting();
    tokio::pin!(waiting);
    let reason = tokio::select! {
        reason = &mut waiting => reason,
        () = shutdown_signal() => {
            cancel.cancel();
            waiting.await
        }
    }
    .context("stdio service task failed")?;

    match reason {
        QuitReason::JoinError(error) => Err(anyhow::anyhow!("stdio service task failed: {error}")),
        reason => {
            tracing::info!(?reason, "stdio session finished");
            Ok(())
        }
    }
}
