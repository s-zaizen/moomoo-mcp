use anyhow::Context;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

use moomoo_mcp::{config::Config, server::MoomooServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().context("failed to load configuration")?;
    let server = MoomooServer::new(config);
    server
        .serve(stdio())
        .await
        .context("failed to start MCP server")?
        .waiting()
        .await
        .context("MCP server task failed")?;
    Ok(())
}
