use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use clap::Parser;
use serde::{de::Error as _, Deserialize, Serialize};
use serde_json::json;
use tokio::{fs, net::TcpListener, time::sleep};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

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

    let _ = tracing_subscriber::fmt().with_env_filter(env_filter).try_init();
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/__health", get(health))
        .route("/__routes", get(list_routes))
        .fallback(dispatch)
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn list_routes(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.summaries.as_ref().clone())
}

async fn dispatch(State(state): State<AppState>, request: Request) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let key = RouteKey {
        method: method.clone(),
        path: path.clone(),
    };

    if let Some(route) = state.routes.get(&key) {
        if let Some(delay) = route.delay {
            sleep(delay).await;
        }

        let mut builder = Response::builder().status(route.status);
        for (name, value) in route.headers.iter() {
            builder = builder.header(name, value);
        }

        let body = route
            .body
            .as_ref()
            .map(|bytes| Body::from(bytes.as_ref().clone()))
            .unwrap_or_else(Body::empty);

        return builder
            .body(body)
            .unwrap_or_else(|error| {
                warn!(?error, "Antwort konnte nicht erstellt werden");
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("response error"))
                    .expect("valid response")
            });
    }

    warn!(method = %method, path, "Kein Mock definiert");
    let payload = json!({
        "error": "not_found",
        "message": "Für diese Kombination aus Methode und Pfad ist kein Mock konfiguriert.",
    });
    (StatusCode::NOT_FOUND, Json(payload)).into_response()
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    routes: Vec<RouteConfig>,
}

impl Config {
    async fn from_file(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path)
            .await
            .with_context(|| format!("Konnte Datei '{}' nicht lesen", path.display()))?;
        let config = serde_json::from_str(&data)
            .with_context(|| format!("Ungültiges JSON in '{}'", path.display()))?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
struct RouteConfig {
    #[serde(deserialize_with = "method_from_str")]
    method: Method,
    path: String,
    #[serde(default = "default_status")]
    status: u16,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Option<serde_json::Value>,
    #[serde(default)]
    text_body: Option<String>,
    #[serde(default)]
    delay_ms: Option<u64>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Clone)]
struct AppState {
    routes: Arc<HashMap<RouteKey, RouteRuntime>>,
    summaries: Arc<Vec<RouteSummary>>,
}

impl TryFrom<Config> for AppState {
    type Error = anyhow::Error;

    fn try_from(config: Config) -> Result<Self> {
        let mut routes = HashMap::new();
        let mut summaries = Vec::new();

        for route in config.routes {
            let method_string = route.method.to_string();
            let path_string = route.path.clone();
            let status = route.status;
            let description = route.description.clone();

            let runtime = RouteRuntime::try_from(&route)?;
            let key = RouteKey {
                method: route.method,
                path: route.path,
            };

            if routes.insert(key, runtime).is_some() {
                warn!(method = %method_string, path = %path_string, "Doppelter Eintrag überschrieben");
            }

            summaries.push(RouteSummary {
                method: method_string,
                path: path_string,
                status,
                description,
            });
        }

        Ok(Self {
            routes: Arc::new(routes),
            summaries: Arc::new(summaries),
        })
    }
}

#[derive(Clone)]
struct RouteRuntime {
    status: StatusCode,
    headers: Arc<Vec<(HeaderName, HeaderValue)>>,
    body: Option<Arc<Vec<u8>>>,
    delay: Option<Duration>,
}

impl RouteRuntime {
    fn try_from(route: &RouteConfig) -> Result<Self> {
        let status = StatusCode::from_u16(route.status)
            .with_context(|| format!("Ungültiger Statuscode {}", route.status))?;

        let mut headers = Vec::new();
        let mut has_content_type = false;
        for (name, value) in &route.headers {
            let header_name: HeaderName = name
                .parse()
                .with_context(|| format!("Ungültiger Header-Name: {name}"))?;
            let header_value: HeaderValue = value
                .parse()
                .with_context(|| format!("Ungültiger Header-Wert für {name}"))?;

            if header_name == CONTENT_TYPE {
                has_content_type = true;
            }

            headers.push((header_name, header_value));
        }

        let (body_bytes, default_ct): (Option<Vec<u8>>, Option<&'static str>) = match (&route.body, &route.text_body) {
            (Some(_), Some(_)) => bail!(
                "Route {} {} definiert sowohl 'body' als auch 'text_body'. Bitte nur eines verwenden.",
                route.method,
                route.path
            ),
            (Some(body), None) => {
                let bytes = serde_json::to_vec(body)
                    .context("Antwort-Body konnte nicht serialisiert werden")?;
                (Some(bytes), Some("application/json"))
            }
            (None, Some(text)) => (Some(text.clone().into_bytes()), Some("text/plain; charset=utf-8")),
            (None, None) => (None, None),
        };

        let body_bytes = body_bytes.map(|bytes| Arc::new(bytes));

        if body_bytes.is_some() && !has_content_type {
            if let Some(default) = default_ct {
                headers.push((CONTENT_TYPE, HeaderValue::from_static(default)));
            }
        }

        let delay = route.delay_ms.map(Duration::from_millis);

        Ok(Self {
            status,
            headers: Arc::new(headers),
            body: body_bytes,
            delay,
        })
    }
}

#[derive(Hash, Eq, PartialEq)]
struct RouteKey {
    method: Method,
    path: String,
}

#[derive(Clone, Serialize)]
struct RouteSummary {
    method: String,
    path: String,
    status: u16,
    description: Option<String>,
}

fn default_status() -> u16 {
    200
}

fn method_from_str<'de, D>(deserializer: D) -> Result<Method, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    raw.parse::<Method>().map_err(D::Error::custom)
}
