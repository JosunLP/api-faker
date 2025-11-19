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
use url::form_urlencoded;

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

    if let Some(candidates) = state.routes.get(&key) {
        let query_map = request.uri().query().map(parse_query_map);
        let matching_route = candidates
            .iter()
            .filter(|candidate| candidate.query_params.is_some())
            .find(|candidate| candidate.matches_query(query_map.as_ref()))
            .or_else(|| {
                candidates
                    .iter()
                    .filter(|candidate| candidate.query_params.is_none())
                    .find(|candidate| candidate.matches_query(query_map.as_ref()))
            });

        if let Some(route) = matching_route {
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
    query: BTreeMap<String, String>,
    #[serde(default)]
    delay_ms: Option<u64>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    variants: Vec<RouteVariantConfig>,
}

#[derive(Debug, Deserialize)]
struct RouteVariantConfig {
    #[serde(default)]
    query: BTreeMap<String, String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Option<serde_json::Value>,
    #[serde(default)]
    text_body: Option<String>,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    delay_ms: Option<u64>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Clone)]
struct AppState {
    routes: Arc<HashMap<RouteKey, Vec<RouteRuntime>>>,
    summaries: Arc<Vec<RouteSummary>>,
}

impl TryFrom<Config> for AppState {
    type Error = anyhow::Error;

    fn try_from(config: Config) -> Result<Self> {
        let mut routes: HashMap<RouteKey, Vec<RouteRuntime>> = HashMap::new();
        let mut summaries = Vec::new();

        for route in config.routes {
            let method_string = route.method.to_string();
            let path_string = route.path.clone();
            let key = RouteKey {
                method: route.method.clone(),
                path: route.path.clone(),
            };

            let mut push_runtime = |runtime: RouteRuntime,
                                    description: Option<String>,
                                    query_summary: Option<BTreeMap<String, String>>| {
                let summary_status = runtime.status.as_u16();
                let entry = routes.entry(key.clone()).or_insert_with(Vec::new);
                if let Some(existing) = entry
                    .iter_mut()
                    .find(|existing| existing.same_query_signature(&runtime))
                {
                    warn!(method = %method_string, path = %path_string, "Doppelter Eintrag mit identischen Query-Parametern überschrieben");
                    *existing = runtime;
                } else {
                    entry.push(runtime);
                }

                summaries.push(RouteSummary {
                    method: method_string.clone(),
                    path: path_string.clone(),
                    status: summary_status,
                    description,
                    query: query_summary,
                });
            };

            let base_query_summary = if route.query.is_empty() {
                None
            } else {
                Some(route.query.clone())
            };
            let base_description = route.description.clone();
            let base_runtime = RouteRuntime::from_config(&route, None)?;
            push_runtime(base_runtime, base_description, base_query_summary);

            for variant in &route.variants {
                let variant_description = variant
                    .description
                    .clone()
                    .or_else(|| route.description.clone());
                let variant_query = if variant.query.is_empty() {
                    if route.query.is_empty() {
                        None
                    } else {
                        Some(route.query.clone())
                    }
                } else {
                    Some(variant.query.clone())
                };

                let runtime = RouteRuntime::from_config(&route, Some(variant))?;
                push_runtime(runtime, variant_description, variant_query);
            }
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
    query_params: Option<Arc<Vec<(String, String)>>>,
}

impl RouteRuntime {
    fn from_config(route: &RouteConfig, variant: Option<&RouteVariantConfig>) -> Result<Self> {
        let status_raw = variant.and_then(|v| v.status).unwrap_or(route.status);
        let status = StatusCode::from_u16(status_raw)
            .with_context(|| format!("Ungültiger Statuscode {}", status_raw))?;

        let mut header_map = route.headers.clone();
        if let Some(variant) = variant {
            for (name, value) in &variant.headers {
                header_map.insert(name.clone(), value.clone());
            }
        }

        let mut headers = Vec::new();
        let mut has_content_type = false;
        for (name, value) in header_map {
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

        let body_value = variant
            .and_then(|v| v.body.as_ref())
            .or_else(|| route.body.as_ref());
        let text_value = variant
            .and_then(|v| v.text_body.as_ref())
            .or_else(|| route.text_body.as_ref());

        let (body_bytes, default_ct): (Option<Vec<u8>>, Option<&'static str>) = match (body_value, text_value) {
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
            (None, Some(text)) => (Some(text.as_bytes().to_vec()), Some("text/plain; charset=utf-8")),
            (None, None) => (None, None),
        };

        let body_bytes = body_bytes.map(|bytes| Arc::new(bytes));

        if body_bytes.is_some() && !has_content_type {
            if let Some(default) = default_ct {
                headers.push((CONTENT_TYPE, HeaderValue::from_static(default)));
            }
        }

        let delay = variant
            .and_then(|v| v.delay_ms)
            .or(route.delay_ms)
            .map(Duration::from_millis);

        let query_source = match variant {
            Some(variant) if !variant.query.is_empty() => Some(&variant.query),
            _ if !route.query.is_empty() => Some(&route.query),
            _ => None,
        };

        let query_params = query_source.map(|map| {
            Arc::new(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        });

        Ok(Self {
            status,
            headers: Arc::new(headers),
            body: body_bytes,
            delay,
            query_params,
        })
    }

    fn same_query_signature(&self, other: &RouteRuntime) -> bool {
        match (&self.query_params, &other.query_params) {
            (None, None) => true,
            (Some(a), Some(b)) => a.as_slice() == b.as_slice(),
            _ => false,
        }
    }

    fn matches_query(&self, query: Option<&BTreeMap<String, Vec<String>>>) -> bool {
        match &self.query_params {
            None => true,
            Some(expected) => {
                let actual = match query {
                    Some(map) => map,
                    None => return false,
                };
                expected.iter().all(|(key, value)| {
                    actual
                        .get(key)
                        .map(|vals| vals.iter().any(|candidate| candidate == value))
                        .unwrap_or(false)
                })
            }
        }
    }
}

#[derive(Clone, Hash, Eq, PartialEq)]
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
    query: Option<BTreeMap<String, String>>,
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

fn parse_query_map(raw: &str) -> BTreeMap<String, Vec<String>> {
    form_urlencoded::parse(raw.as_bytes())
        .into_owned()
        .fold(BTreeMap::new(), |mut acc, (key, value)| {
            acc.entry(key).or_default().push(value);
            acc
        })
}
