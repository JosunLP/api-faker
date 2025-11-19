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
    /// Pfad zur Konfigurationsdatei mit den Fake-Endpunkten
    #[arg(short, long, default_value = "mock_endpoints.json")]
    config: PathBuf,

    /// Host-Adresse, auf der der Server lauscht
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port, auf dem der Server lauscht
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();

    let config = Config::from_file(&cli.config).await?;
    let state = AppState::try_from(config)?;
    let router = build_router(state);

    let addr: SocketAddr = format!("{}:{}", cli.host, cli.port)
        .parse()
        .context("Ungültige Host/Port-Kombination")?;

    info!(%addr, "Mock API läuft");

    let listener = TcpListener::bind(addr)
        .await
        .context("Konnte Socket nicht binden")?;
    axum::serve(listener, router)
        .await
        .context("Serverlauf fehlgeschlagen")?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("api_faker=info,axum=info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();
}
