use std::{io::ErrorKind, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use tokio::net::TcpListener;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod config;
mod runtime;
mod server;
mod update;

use crate::config::Config;
use crate::server::{AppState, build_router};

#[derive(Parser, Debug)]
#[command(author, version, about = "Serve fake API endpoints from a JSON file.", long_about = None)]
struct Cli {
    /// Path to the configuration file containing mock endpoints
    #[arg(short, long, default_value = "mock_endpoints.json", env = "API_FAKER_CONFIG", value_hint = clap::ValueHint::FilePath)]
    config: PathBuf,

    /// Host interface the server should bind to
    #[arg(long, env = "API_FAKER_HOST")]
    host: Option<String>,

    /// Port the server should listen on
    #[arg(short, long, env = "API_FAKER_PORT")]
    port: Option<u16>,

    /// Highest port to probe when the desired port is already in use
    #[arg(
        long,
        default_value_t = 65535,
        env = "API_FAKER_PORT_MAX",
        value_name = "PORT"
    )]
    port_max: u16,

    /// Override the log level for API Faker and Axum (error, warn, info, debug, trace)
    #[arg(long, value_enum, env = "API_FAKER_LOG", value_name = "LEVEL")]
    log_level: Option<LogLevel>,

    /// Validate the configuration and exit without starting the server
    #[arg(long)]
    dry_run: bool,

    /// Check for updates without installing
    #[arg(long)]
    check: bool,

    /// Update to the latest version
    #[arg(long)]
    update: bool,
}

#[derive(Clone, Debug, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn as_directive(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log_level.as_ref());

    // Handle update commands first
    if cli.check {
        match update::check_for_updates().await? {
            Some(new_version) => {
                info!("New version available: {}", new_version);
                info!("Run 'api-faker --update' to install it");
            }
            None => {
                info!(
                    "You are running the latest version ({})",
                    env!("CARGO_PKG_VERSION")
                );
            }
        }
        return Ok(());
    }

    if cli.update {
        return update::perform_update().await;
    }

    let config = Config::from_file(&cli.config).await?;
    let route_count = config.routes.len();
    let server_settings = config.server.clone();
    let state = AppState::try_from(config)?;

    if cli.dry_run {
        info!(
            routes = route_count,
            config = %cli.config.display(),
            "Configuration validated successfully (--dry-run)"
        );
        return Ok(());
    }

    let router = build_router(state);

    let host = cli
        .host
        .or_else(|| server_settings.host.clone())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = cli.port.or(server_settings.port).unwrap_or(8080);

    let (listener, addr) = bind_with_fallback(&host, port, cli.port_max).await?;

    info!(%addr, "Mock API running");

    axum::serve(listener, router)
        .await
        .context("Server execution failed")?;

    Ok(())
}

fn init_tracing(cli_level: Option<&LogLevel>) {
    let env_filter = if let Some(level) = cli_level {
        let directive = level.as_directive();
        EnvFilter::new(format!("api_faker={directive},axum={directive}"))
    } else {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("api_faker=info,axum=info"))
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();
}

async fn bind_with_fallback(
    host: &str,
    start_port: u16,
    max_port: u16,
) -> Result<(TcpListener, SocketAddr)> {
    if start_port > max_port {
        bail!("Requested port {start_port} is higher than the allowed maximum {max_port}");
    }

    let mut port = start_port;

    loop {
        if port > max_port {
            bail!(
                "No available port between {start_port} and {max_port}. Consider specifying --port-max"
            );
        }

        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .context("Invalid host/port combination")?;

        match TcpListener::bind(addr).await {
            Ok(listener) => return Ok((listener, addr)),
            Err(error) if error.kind() == ErrorKind::AddrInUse => {
                if port == max_port {
                    bail!(
                        "Port {port} is in use and no higher ports are permitted (--port-max={max_port})"
                    );
                }
                warn!(%addr, next_port = port + 1, "Port unavailable, trying next port");
                port = port.saturating_add(1);
            }
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                return Err(error).with_context(|| format!(
                    "Permission denied while binding to {addr}. Try a different port or run with elevated privileges."
                ));
            }
            Err(error) => return Err(error).context("Failed to bind socket"),
        }
    }
}
