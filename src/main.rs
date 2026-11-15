mod api;
mod auth;
mod cache;
mod config;
mod markdown;
mod metrics;
mod model;
mod providers;
mod server;
mod signal;
mod stdio;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "atlassian-mcp",
    version,
    about = "Read-only MCP server for Atlassian Jira and Confluence"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve MCP over Streamable HTTP at /mcp (default)
    Serve,
    /// Serve MCP over stdin/stdout for a client that launches this process
    Stdio,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cmd = Cli::parse().cmd.unwrap_or(Cmd::Serve);

    // Logs always go to stderr: in stdio mode stdout is reserved for MCP messages.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(matches!(cmd, Cmd::Serve))
        .init();

    match cmd {
        Cmd::Serve => {
            api::serve(config::Config::from_env()?, config::HttpConfig::from_env()?).await
        }
        Cmd::Stdio => stdio::run(config::Config::from_env()?).await,
    }
}
