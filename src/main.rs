use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod config;
mod runtime;
mod server;

use crate::config::Config;
use crate::server::{AppState, build_router};

#[derive(Parser, Debug)]
#[command(author, version, about = "Serve fake API endpoints from a JSON file.")]
struct Cli {
    /// Path to the configuration file containing mock endpoints
    #[arg(short, long, default_value = "mock_endpoints.json")]
    config: PathBuf,

    /// Host interface the server should bind to
    #[arg(long)]
    host: Option<String>,

    /// Port the server should listen on
    #[arg(short, long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();

    let config = Config::from_file(&cli.config).await?;
    let server_settings = config.server.clone();
    let state = AppState::try_from(config)?;
    let router = build_router(state);

    let host = cli
        .host
        .or_else(|| server_settings.host.clone())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = cli.port.or(server_settings.port).unwrap_or(8080);

    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .context("Invalid host/port combination")?;

    info!(%addr, "Mock API running");

    let listener = TcpListener::bind(addr)
        .await
        .context("Failed to bind socket")?;
    axum::serve(listener, router)
        .await
        .context("Server execution failed")?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("api_faker=info,axum=info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();
}
