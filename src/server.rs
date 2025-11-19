use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::json;
use tokio::time::sleep;
use tracing::warn;
use url::form_urlencoded;

use crate::config::Config;
use crate::runtime::{RouteRuntime, VariantSource};

const ERROR_QUERY_PARAM: &str = "__error";
const ERROR_HEADER_NAME: &str = "x-api-faker-error";

pub fn build_router(state: AppState) -> Router {
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
    if let Some(delay) = route.delay() {
        sleep(delay).await;
    }

    let mut builder = Response::builder().status(route.status);
    for (name, value) in route.headers() {
        builder = builder.header(name, value);
    }

    let body = route.body().map(Body::from).unwrap_or_else(Body::empty);

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

#[derive(Clone)]
pub struct AppState {
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
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
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
    use crate::config::{RouteConfig, RouteErrorVariantConfig};
    use axum::http::Method;

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
