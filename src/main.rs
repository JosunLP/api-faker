use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
    routing::get,
};
use clap::Parser;
use serde::{Deserialize, Serialize, de::Error as _, ser::Serializer};
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

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/__health", get(health))
        .route("/__routes", get(list_routes))
        .fallback(dispatch)
        .with_state(state)
}

const ERROR_QUERY_PARAM: &str = "__error";
const ERROR_HEADER_NAME: &str = "x-api-faker-error";

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn list_routes(State(state): State<AppState>) -> impl IntoResponse {
    Json(RouteSummariesResponse(Arc::clone(&state.summaries)))
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
        let error_trigger = extract_error_trigger(request.headers(), query_map.as_ref());

        if let Some(trigger) = error_trigger.as_deref() {
            if let Some(route) = candidates
                .iter()
                .find(|candidate| candidate.error_trigger() == Some(trigger))
            {
                return respond_from_runtime(route).await;
            }
        }

        let matching_route = candidates
            .iter()
            .filter(|candidate| !candidate.is_error())
            .filter(|candidate| candidate.matches_query(query_map.as_ref()))
            .max_by_key(|candidate| candidate.specificity_rank());

        if let Some(route) = matching_route {
            return respond_from_runtime(route).await;
        }
    }

    warn!(method = %method, path, "Kein Mock definiert");
    let payload = json!({
        "error": "not_found",
        "message": "Für diese Kombination aus Methode und Pfad ist kein Mock konfiguriert.",
    });
    (StatusCode::NOT_FOUND, Json(payload)).into_response()
}

async fn respond_from_runtime(route: &RouteRuntime) -> Response {
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
        .map(|bytes| Body::from(bytes.clone()))
        .unwrap_or_else(Body::empty);

    builder.body(body).unwrap_or_else(|error| {
        warn!(?error, "Antwort konnte nicht erstellt werden");
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("response error"))
            .expect("valid response")
    })
}

fn extract_error_trigger(
    headers: &HeaderMap,
    query: Option<&BTreeMap<String, Vec<String>>>,
) -> Option<String> {
    if let Some(value) = headers
        .get(ERROR_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Some(value.to_string());
    }

    query
        .and_then(|map| map.get(ERROR_QUERY_PARAM))
        .and_then(|values| values.last())
        .map(|value| value.to_string())
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
    #[serde(default)]
    error_variants: Vec<RouteErrorVariantConfig>,
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

#[derive(Debug, Deserialize)]
struct RouteErrorVariantConfig {
    name: String,
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

            let mut push_runtime =
                |runtime: RouteRuntime,
                 description: Option<String>,
                 query_summary: Option<BTreeMap<String, String>>| {
                    let summary_status = runtime.status.as_u16();
                    let summary_error = runtime.error_trigger().map(str::to_string);
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
                        error_trigger: summary_error,
                    });
                };

            let base_query_summary = if route.query.is_empty() {
                None
            } else {
                Some(route.query.clone())
            };
            let base_description = route.description.clone();
            let base_runtime = RouteRuntime::from_source(&route, VariantSource::Base)?;
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

                let runtime = RouteRuntime::from_source(&route, VariantSource::Query(variant))?;
                push_runtime(runtime, variant_description, variant_query);
            }

            for error_variant in &route.error_variants {
                let description = error_variant
                    .description
                    .clone()
                    .or_else(|| route.description.clone());
                let runtime =
                    RouteRuntime::from_source(&route, VariantSource::Error(error_variant))?;
                push_runtime(runtime, description, None);
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
    body: Option<Bytes>,
    delay: Option<Duration>,
    query_params: Option<Arc<Vec<(String, String)>>>,
    error_trigger: Option<Arc<String>>,
}

#[derive(Clone, Copy)]
enum VariantSource<'a> {
    Base,
    Query(&'a RouteVariantConfig),
    Error(&'a RouteErrorVariantConfig),
}

impl RouteRuntime {
    fn from_source(route: &RouteConfig, source: VariantSource<'_>) -> Result<Self> {
        let status_raw = match source {
            VariantSource::Base => route.status,
            VariantSource::Query(variant) => variant.status.unwrap_or(route.status),
            VariantSource::Error(variant) => variant.status.unwrap_or(route.status),
        };

        let status = StatusCode::from_u16(status_raw)
            .with_context(|| format!("Ungültiger Statuscode {}", status_raw))?;

        let mut header_map = route.headers.clone();
        match source {
            VariantSource::Query(variant) => {
                for (name, value) in &variant.headers {
                    header_map.insert(name.clone(), value.clone());
                }
            }
            VariantSource::Error(variant) => {
                for (name, value) in &variant.headers {
                    header_map.insert(name.clone(), value.clone());
                }
            }
            VariantSource::Base => {}
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

        enum BodySpec<'a> {
            Json(&'a serde_json::Value),
            Text(&'a str),
            None,
        }

        let body_spec = match source {
            VariantSource::Base => {
                if route.body.is_some() && route.text_body.is_some() {
                    bail!(
                        "Route {} {} definiert sowohl 'body' als auch 'text_body'. Bitte nur eines verwenden.",
                        route.method,
                        route.path
                    );
                }
                if let Some(body) = route.body.as_ref() {
                    BodySpec::Json(body)
                } else if let Some(text) = route.text_body.as_ref() {
                    BodySpec::Text(text)
                } else {
                    BodySpec::None
                }
            }
            VariantSource::Query(variant) => {
                if variant.body.is_some() && variant.text_body.is_some() {
                    bail!(
                        "Variante von {} {} definiert sowohl 'body' als auch 'text_body'. Bitte nur eines verwenden.",
                        route.method,
                        route.path
                    );
                }
                if let Some(body) = variant.body.as_ref() {
                    BodySpec::Json(body)
                } else if let Some(text) = variant.text_body.as_ref() {
                    BodySpec::Text(text)
                } else if let Some(body) = route.body.as_ref() {
                    BodySpec::Json(body)
                } else if let Some(text) = route.text_body.as_ref() {
                    BodySpec::Text(text)
                } else {
                    BodySpec::None
                }
            }
            VariantSource::Error(variant) => {
                if variant.body.is_some() && variant.text_body.is_some() {
                    bail!(
                        "Fehlervariante '{}' für {} {} definiert sowohl 'body' als auch 'text_body'. Bitte nur eines verwenden.",
                        variant.name,
                        route.method,
                        route.path
                    );
                }
                if let Some(body) = variant.body.as_ref() {
                    BodySpec::Json(body)
                } else if let Some(text) = variant.text_body.as_ref() {
                    BodySpec::Text(text)
                } else if let Some(body) = route.body.as_ref() {
                    BodySpec::Json(body)
                } else if let Some(text) = route.text_body.as_ref() {
                    BodySpec::Text(text)
                } else {
                    BodySpec::None
                }
            }
        };

        let (body_bytes, default_ct): (Option<Bytes>, Option<&'static str>) = match body_spec {
            BodySpec::Json(body) => {
                let bytes = serde_json::to_vec(body)
                    .context("Antwort-Body konnte nicht serialisiert werden")?;
                (Some(Bytes::from(bytes)), Some("application/json"))
            }
            BodySpec::Text(text) => {
                let bytes = Bytes::from(text.to_owned());
                (Some(bytes), Some("text/plain; charset=utf-8"))
            }
            BodySpec::None => (None, None),
        };

        if body_bytes.is_some() && !has_content_type {
            if let Some(default) = default_ct {
                headers.push((CONTENT_TYPE, HeaderValue::from_static(default)));
            }
        }

        let delay = match source {
            VariantSource::Base => route.delay_ms,
            VariantSource::Query(variant) => variant.delay_ms.or(route.delay_ms),
            VariantSource::Error(variant) => variant.delay_ms.or(route.delay_ms),
        }
        .map(Duration::from_millis);

        let query_params = match source {
            VariantSource::Error(_) => None,
            VariantSource::Base => map_query_arc(&route.query),
            VariantSource::Query(variant) => {
                if !variant.query.is_empty() {
                    map_query_arc(&variant.query)
                } else {
                    map_query_arc(&route.query)
                }
            }
        };

        let error_trigger = match source {
            VariantSource::Error(variant) => {
                let trimmed = variant.name.trim();
                if trimmed.is_empty() {
                    bail!(
                        "Fehlervariante für {} {} benötigt ein nicht-leeres 'name'-Feld.",
                        route.method,
                        route.path
                    );
                }
                Some(Arc::new(trimmed.to_string()))
            }
            _ => None,
        };

        Ok(Self {
            status,
            headers: Arc::new(headers),
            body: body_bytes,
            delay,
            query_params,
            error_trigger,
        })
    }

    fn same_query_signature(&self, other: &RouteRuntime) -> bool {
        if self.error_trigger.as_deref() != other.error_trigger.as_deref() {
            return false;
        }

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

    fn specificity_rank(&self) -> u8 {
        if self.query_params.is_some() { 1 } else { 0 }
    }

    fn error_trigger(&self) -> Option<&str> {
        self.error_trigger.as_deref().map(|s| s.as_str())
    }

    fn is_error(&self) -> bool {
        self.error_trigger.is_some()
    }
}

fn map_query_arc(map: &BTreeMap<String, String>) -> Option<Arc<Vec<(String, String)>>> {
    if map.is_empty() {
        None
    } else {
        Some(Arc::new(
            map.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>(),
        ))
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
    error_trigger: Option<String>,
}

#[derive(Clone)]
struct RouteSummariesResponse(Arc<Vec<RouteSummary>>);

impl Serialize for RouteSummariesResponse {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
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
    form_urlencoded::parse(raw.as_bytes()).into_owned().fold(
        BTreeMap::new(),
        |mut acc, (key, value)| {
            acc.entry(key).or_default().push(value);
            acc
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_text_body_overrides_base_json() {
        let route = RouteConfig {
            method: Method::GET,
            path: "/users".into(),
            status: 200,
            headers: BTreeMap::new(),
            body: Some(json!({"foo": "bar"})),
            text_body: None,
            query: BTreeMap::new(),
            delay_ms: None,
            description: None,
            variants: Vec::new(),
            error_variants: Vec::new(),
        };

        let variant = RouteVariantConfig {
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            body: None,
            text_body: Some("hello world".into()),
            status: None,
            delay_ms: None,
            description: None,
        };

        let runtime =
            RouteRuntime::from_source(&route, VariantSource::Query(&variant)).expect("runtime");
        let body = runtime.body.clone().expect("body");
        assert_eq!(body, Bytes::from_static(b"hello world"));

        assert!(
            runtime
                .headers
                .iter()
                .any(|(name, value)| name == &CONTENT_TYPE && value == "text/plain; charset=utf-8")
        );
    }

    #[test]
    fn error_variant_inherits_base_headers() {
        let mut base_headers = BTreeMap::new();
        base_headers.insert("x-base".into(), "foo".into());

        let route = RouteConfig {
            method: Method::GET,
            path: "/users".into(),
            status: 200,
            headers: base_headers,
            body: Some(json!({"foo": "bar"})),
            text_body: None,
            query: BTreeMap::new(),
            delay_ms: None,
            description: None,
            variants: Vec::new(),
            error_variants: Vec::new(),
        };

        let error_variant = RouteErrorVariantConfig {
            name: "boom".into(),
            headers: BTreeMap::new(),
            body: None,
            text_body: Some("kaputt".into()),
            status: Some(503),
            delay_ms: None,
            description: None,
        };

        let runtime = RouteRuntime::from_source(&route, VariantSource::Error(&error_variant))
            .expect("runtime");
        assert_eq!(runtime.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(runtime.error_trigger(), Some("boom"));
        assert!(
            runtime
                .headers
                .iter()
                .any(|(name, value)| name == &CONTENT_TYPE && value == "text/plain; charset=utf-8")
        );
    }

    #[tokio::test]
    async fn dispatch_prefers_error_variant_when_triggered() {
        let route = RouteConfig {
            method: Method::GET,
            path: "/users".into(),
            status: 200,
            headers: BTreeMap::new(),
            body: None,
            text_body: Some("ok".into()),
            query: BTreeMap::new(),
            delay_ms: None,
            description: None,
            variants: Vec::new(),
            error_variants: vec![RouteErrorVariantConfig {
                name: "fail".into(),
                headers: BTreeMap::new(),
                body: None,
                text_body: Some("nope".into()),
                status: Some(500),
                delay_ms: None,
                description: None,
            }],
        };

        let config = Config {
            routes: vec![route],
        };
        let state = AppState::try_from(config).expect("state");

        let request = Request::builder()
            .method(Method::GET)
            .uri("http://localhost/users?__error=fail")
            .body(Body::empty())
            .expect("request");

        let response = dispatch(State(state.clone()), request).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
