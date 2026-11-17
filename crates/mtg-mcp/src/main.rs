//! `mtg-mcp`: serve the engine over stdio (default) or streamable HTTP.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use mtg_mcp::{Games, MtgServer};
use rmcp::ServiceExt;
use rmcp::transport::stdio;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

#[derive(Parser)]
#[command(name = "mtg-mcp", about = "MCP server for the MTG engine")]
struct Cli {
    /// Serve streamable HTTP at this address (e.g. 127.0.0.1:8765) instead
    /// of stdio; the MCP endpoint is /mcp.
    #[arg(long)]
    http: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let games = Arc::new(Games::new(Arc::new(mtg_dsl::builtin_pool())));
    match cli.http {
        None => {
            let service = MtgServer::new(games).serve(stdio()).await?;
            service.waiting().await?;
        }
        Some(addr) => {
            let service = StreamableHttpService::new(
                move || Ok(MtgServer::new(games.clone())),
                Arc::new(LocalSessionManager::default()),
                StreamableHttpServerConfig::default(),
            );
            let router = axum::Router::new().nest_service("/mcp", service);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            tracing::info!("serving MCP at http://{addr}/mcp");
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
    }
    Ok(())
}
