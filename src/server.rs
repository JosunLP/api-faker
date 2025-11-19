use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use tokio::time::sleep;
use tracing::warn;
use url::form_urlencoded;

use crate::config::{Config, RouteConfig};
use crate::runtime::{RouteRuntime, VariantSource};

const ERROR_QUERY_PARAM: &str = "__error";
const ERROR_HEADER_NAME: &str = "x-api-faker-error";
const SWAGGER_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8" />
        <title>API Faker · Swagger UI</title>
        <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
        <style>
            body { margin: 0; padding: 0; }
            #swagger-ui { height: 100vh; }
        </style>
    </head>
    <body>
        <div id="swagger-ui"></div>
        <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
        <script>
            window.addEventListener('load', () => {
                window.ui = SwaggerUIBundle({
                    url: '/__openapi.json',
                    dom_id: '#swagger-ui',
                    presets: [SwaggerUIBundle.presets.apis],
                    layout: 'BaseLayout'
                });
            });
        </script>
    </body>
</html>"#;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/__health", get(health))
        .route("/__routes", get(list_routes))
        .route("/__openapi.json", get(openapi_spec))
        .route("/__swagger", get(swagger_ui))
        .fallback(dispatch)
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn list_routes(State(state): State<AppState>) -> impl IntoResponse {
    Json(RouteSummariesResponse(Arc::clone(&state.summaries)))
}

async fn openapi_spec(State(state): State<AppState>) -> impl IntoResponse {
    Json((*state.openapi).clone())
}

async fn swagger_ui() -> impl IntoResponse {
    Html(SWAGGER_UI_HTML)
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

        if let Some(trigger) = error_trigger.as_deref()
            && let Some(route) = candidates
                .iter()
                .find(|candidate| candidate.error_trigger() == Some(trigger))
        {
            return respond_from_runtime(route).await;
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
    openapi: Arc<Value>,
}

impl TryFrom<Config> for AppState {
    type Error = anyhow::Error;

    fn try_from(config: Config) -> Result<Self> {
        let mut routes: HashMap<RouteKey, Vec<RouteRuntime>> = HashMap::new();
        let mut summaries = Vec::new();
        let mut openapi_builder = OpenApiBuilder::new();

        for route in config.routes {
            openapi_builder.record_route(&route);
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
                    let entry = routes.entry(key.clone()).or_default();
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

        let openapi = openapi_builder.finish();

        Ok(Self {
            routes: Arc::new(routes),
            summaries: Arc::new(summaries),
            openapi: Arc::new(openapi),
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

struct OpenApiBuilder {
    paths: Map<String, Value>,
}

impl OpenApiBuilder {
    fn new() -> Self {
        Self { paths: Map::new() }
    }

    fn record_route(&mut self, route: &RouteConfig) {
        let method_key = route.method.as_str().to_ascii_lowercase();
        let operation = Value::Object(Self::operation_for_route(route));
        let entry = self
            .paths
            .entry(route.path.clone())
            .or_insert_with(|| Value::Object(Map::new()));

        let path_obj = entry
            .as_object_mut()
            .expect("Pfad-Eintrag muss ein Objekt sein");
        path_obj.insert(method_key, operation);
    }

    fn operation_for_route(route: &RouteConfig) -> Map<String, Value> {
        let mut op = Map::new();
        if let Some(description) = &route.description {
            op.insert("summary".into(), Value::String(description.clone()));
        }

        op.insert(
            "operationId".into(),
            Value::String(build_operation_id(route)),
        );

        let mut parameters = build_query_parameters(route);
        parameters.extend(build_error_injection_parameters());
        if !parameters.is_empty() {
            op.insert("parameters".into(), Value::Array(parameters));
        }

        op.insert("responses".into(), Value::Object(build_responses(route)));

        if let Some(vendor) = build_vendor_extension(route) {
            op.insert("x-api-faker".into(), vendor);
        }

        op
    }

    fn finish(self) -> Value {
        let mut root = Map::new();
        root.insert("openapi".into(), Value::String("3.1.0".into()));
        root.insert(
            "info".into(),
            json!({
                "title": "API Faker",
                "version": env!("CARGO_PKG_VERSION"),
                "description": "Automatisch generierte OpenAPI-Dokumentation basierend auf mock_endpoints.json"
            }),
        );
        root.insert("servers".into(), json!([{ "url": "/" }]));
        root.insert("paths".into(), Value::Object(self.paths));
        Value::Object(root)
    }
}

fn build_operation_id(route: &RouteConfig) -> String {
    let method = route.method.as_str().to_ascii_lowercase();
    let sanitized_path: String = route
        .path
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    format!("{}_{}", method, sanitized_path)
}

fn build_query_parameters(route: &RouteConfig) -> Vec<Value> {
    let mut params = Vec::new();
    let mut optional_seen = BTreeSet::new();

    for (name, example) in &route.query {
        params.push(json!({
            "name": name,
            "in": "query",
            "required": true,
            "schema": {
                "type": "string",
                "example": example,
            }
        }));
        optional_seen.insert(name.clone());
    }

    for variant in &route.variants {
        for (name, example) in &variant.query {
            if optional_seen.contains(name) {
                continue;
            }
            params.push(json!({
                "name": name,
                "in": "query",
                "required": false,
                "schema": {
                    "type": "string",
                    "example": example,
                }
            }));
            optional_seen.insert(name.clone());
        }
    }

    params
}

fn build_error_injection_parameters() -> Vec<Value> {
    vec![
        json!({
            "name": ERROR_QUERY_PARAM,
            "in": "query",
            "required": false,
            "description": "Aktiviert eine konfigurierte Fehlervariante via Query-Parameter",
            "schema": {
                "type": "string"
            }
        }),
        json!({
            "name": ERROR_HEADER_NAME,
            "in": "header",
            "required": false,
            "description": "Aktiviert eine konfigurierte Fehlervariante via HTTP-Header",
            "schema": {
                "type": "string"
            }
        }),
    ]
}

fn build_responses(route: &RouteConfig) -> Map<String, Value> {
    let mut responses = Map::new();
    let description = route
        .description
        .clone()
        .unwrap_or_else(|| format!("{} {} Antwort", route.method, route.path));
    responses.insert(
        route.status.to_string(),
        build_response_object(&description, route.body.as_ref(), route.text_body.as_ref()),
    );
    responses
}

fn build_response_object(
    description: &str,
    json_body: Option<&serde_json::Value>,
    text_body: Option<&String>,
) -> Value {
    let mut response = Map::new();
    response.insert("description".into(), Value::String(description.to_string()));
    if let Some(content) = build_content_map(json_body, text_body) {
        response.insert("content".into(), Value::Object(content));
    }
    Value::Object(response)
}

fn build_content_map(
    json_body: Option<&serde_json::Value>,
    text_body: Option<&String>,
) -> Option<Map<String, Value>> {
    let mut content = Map::new();
    if let Some(body) = json_body {
        content.insert("application/json".into(), json!({ "example": body }));
    }
    if let Some(text) = text_body {
        content.insert("text/plain".into(), json!({ "example": text }));
    }
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

fn build_vendor_extension(route: &RouteConfig) -> Option<Value> {
    let mut vendor = Map::new();
    if !route.headers.is_empty() {
        vendor.insert("headers".into(), json!(&route.headers));
    }
    if !route.query.is_empty() {
        vendor.insert("query".into(), json!(&route.query));
    }
    if let Some(delay) = route.delay_ms {
        vendor.insert("delayMs".into(), json!(delay));
    }
    if !route.variants.is_empty() {
        let variants = route
            .variants
            .iter()
            .map(|variant| {
                json!({
                    "description": variant.description,
                    "query": &variant.query,
                    "headers": &variant.headers,
                    "status": variant.status,
                    "delay_ms": variant.delay_ms,
                    "body": &variant.body,
                    "text_body": &variant.text_body,
                })
            })
            .collect();
        vendor.insert("variants".into(), Value::Array(variants));
    }
    if !route.error_variants.is_empty() {
        let error_variants = route
            .error_variants
            .iter()
            .map(|variant| {
                json!({
                    "name": variant.name,
                    "description": variant.description,
                    "headers": &variant.headers,
                    "status": variant.status,
                    "delay_ms": variant.delay_ms,
                    "body": &variant.body,
                    "text_body": &variant.text_body,
                })
            })
            .collect();
        vendor.insert("errorVariants".into(), Value::Array(error_variants));
    }

    if vendor.is_empty() {
        None
    } else {
        Some(Value::Object(vendor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RouteConfig, RouteErrorVariantConfig, RouteVariantConfig};
    use axum::http::Method;
    use serde_json::json;

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

    #[test]
    fn openapi_document_contains_route() {
        let route = RouteConfig {
            method: Method::GET,
            path: "/users".into(),
            status: 200,
            headers: BTreeMap::new(),
            body: Some(json!({"users": []})),
            text_body: None,
            query: BTreeMap::new(),
            delay_ms: None,
            description: Some("List users".into()),
            variants: Vec::new(),
            error_variants: Vec::new(),
        };

        let config = Config {
            routes: vec![route],
        };
        let state = AppState::try_from(config).expect("state");
        let spec = state.openapi.as_ref();

        assert!(spec["paths"]["/users"]["get"].is_object());
        assert_eq!(
            spec["paths"]["/users"]["get"]["responses"]["200"]["description"],
            "List users"
        );
    }

    #[test]
    fn openapi_includes_variant_and_error_parameters() {
        let mut variant_query = BTreeMap::new();
        variant_query.insert("team".into(), "platform".into());

        let route = RouteConfig {
            method: Method::GET,
            path: "/users".into(),
            status: 200,
            headers: BTreeMap::new(),
            body: None,
            text_body: None,
            query: BTreeMap::new(),
            delay_ms: None,
            description: Some("List users".into()),
            variants: vec![RouteVariantConfig {
                query: variant_query,
                headers: BTreeMap::new(),
                body: None,
                text_body: None,
                status: None,
                delay_ms: None,
                description: Some("Filtered".into()),
            }],
            error_variants: Vec::new(),
        };

        let config = Config {
            routes: vec![route],
        };
        let state = AppState::try_from(config).expect("state");
        let params = state.openapi["paths"]["/users"]["get"]["parameters"]
            .as_array()
            .expect("parameters array");

        let has_variant_query = params.iter().any(|param| {
            param["name"] == "team" && param["in"] == "query" && param["required"] == false
        });
        assert!(has_variant_query, "variant query parameter missing");

        let has_error_query = params
            .iter()
            .any(|param| param["name"] == ERROR_QUERY_PARAM);
        assert!(has_error_query, "error query parameter missing");

        let has_error_header = params
            .iter()
            .any(|param| param["name"] == ERROR_HEADER_NAME && param["in"] == "header");
        assert!(has_error_header, "error header parameter missing");
    }
}
