use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde_json::json;
use tokio::time::sleep;
use tracing::warn;

use crate::runtime::RouteRuntime;

use super::AppState;
use super::extractors::{
    attach_response_flags, extract_error_trigger, extract_request_flags, parse_query_map,
};
use super::state::{RouteKey, RouteSummariesResponse};

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

pub async fn health(State(state): State<AppState>) -> Response {
    let mut response = Json(json!({ "status": "ok" })).into_response();
    attach_response_flags(&mut response, &state.default_response_flags);
    response
}

pub async fn list_routes(State(state): State<AppState>) -> Response {
    let mut response = Json(RouteSummariesResponse(Arc::clone(&state.summaries))).into_response();
    attach_response_flags(&mut response, &state.default_response_flags);
    response
}

pub async fn openapi_spec(State(state): State<AppState>) -> Response {
    let mut response = Json((*state.openapi).clone()).into_response();
    attach_response_flags(&mut response, &state.default_response_flags);
    response
}

pub async fn swagger_ui(State(state): State<AppState>) -> Response {
    let mut response = Html(SWAGGER_UI_HTML).into_response();
    attach_response_flags(&mut response, &state.default_response_flags);
    response
}

pub async fn dispatch(State(state): State<AppState>, request: Request) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let key = RouteKey {
        method: method.clone(),
        path: path.clone(),
    };

    if let Some(candidates) = state.routes.get(&key) {
        let query_map = request.uri().query().map(parse_query_map);
        let error_trigger = extract_error_trigger(request.headers(), query_map.as_ref());
        let request_flags = extract_request_flags(request.headers(), query_map.as_ref());
        let request_flags_ref = request_flags.as_ref();

        if let Some(trigger) = error_trigger.as_deref()
            && let Some(route) = candidates.iter().find(|candidate| {
                candidate.error_trigger() == Some(trigger)
                    && candidate.matches_request_flags(request_flags_ref)
            })
        {
            return respond_from_runtime(route).await;
        }

        let matching_route = candidates
            .iter()
            .filter(|candidate| !candidate.is_error())
            .filter(|candidate| candidate.matches_query(query_map.as_ref()))
            .filter(|candidate| candidate.matches_request_flags(request_flags_ref))
            .max_by_key(|candidate| candidate.specificity_rank());

        if let Some(route) = matching_route {
            return respond_from_runtime(route).await;
        }
    }

    warn!(method = %method, path, "No mock defined");
    let payload = json!({
        "error": "not_found",
        "message": "No mock configured for this combination of method and path.",
    });
    let mut response = (StatusCode::NOT_FOUND, Json(payload)).into_response();
    attach_response_flags(&mut response, &state.default_response_flags);
    response
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

    let mut response = builder.body(body).unwrap_or_else(|error| {
        warn!(?error, "Failed to build response");
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("response error"))
            .expect("valid response")
    });
    attach_response_flags(&mut response, route.response_flags());
    response
}

#[cfg(test)]
mod tests {
    use super::super::FLAGS_HEADER_NAME;
    use super::*;
    use crate::config::{
        Config, RouteConfig, RouteErrorVariantConfig, RouteVariantConfig, ServerConfig,
    };
    use axum::http::Method;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};

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
            request_flags: BTreeSet::new(),
            response_flags: Vec::new(),
            delay_ms: None,
            description: None,
            variants: Vec::new(),
            error_variants: vec![RouteErrorVariantConfig {
                name: "fail".into(),
                request_flags: BTreeSet::new(),
                headers: BTreeMap::new(),
                body: None,
                text_body: Some("nope".into()),
                status: Some(500),
                delay_ms: None,
                description: None,
                response_flags: Vec::new(),
            }],
        };

        let config = Config {
            server: ServerConfig::default(),
            response_flags: Vec::new(),
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

    #[tokio::test]
    async fn dispatch_respects_request_flags_for_variants() {
        let mut variant_flags = BTreeSet::new();
        variant_flags.insert("beta".into());

        let route = RouteConfig {
            method: Method::GET,
            path: "/feature".into(),
            status: 200,
            headers: BTreeMap::new(),
            body: Some(json!({"state": "stable"})),
            text_body: None,
            query: BTreeMap::new(),
            request_flags: BTreeSet::new(),
            response_flags: vec!["route".into()],
            delay_ms: None,
            description: None,
            variants: vec![RouteVariantConfig {
                query: BTreeMap::new(),
                request_flags: variant_flags,
                headers: BTreeMap::new(),
                body: Some(json!({"state": "beta"})),
                text_body: None,
                status: None,
                delay_ms: None,
                description: None,
                response_flags: vec!["beta".into()],
            }],
            error_variants: Vec::new(),
        };

        let config = Config {
            server: ServerConfig::default(),
            response_flags: vec!["global".into()],
            routes: vec![route],
        };
        let state = AppState::try_from(config).expect("state");

        let beta_request = Request::builder()
            .method(Method::GET)
            .uri("http://localhost/feature")
            .header(FLAGS_HEADER_NAME, "beta")
            .body(Body::empty())
            .expect("request");

        let beta_response = dispatch(State(state.clone()), beta_request).await;
        assert_eq!(beta_response.status(), StatusCode::OK);
        let header = beta_response
            .headers()
            .get(FLAGS_HEADER_NAME)
            .expect("flags header")
            .to_str()
            .expect("header str");
        assert_eq!(header, "global, route, beta");

        let default_request = Request::builder()
            .method(Method::GET)
            .uri("http://localhost/feature")
            .body(Body::empty())
            .expect("request");
        let default_response = dispatch(State(state), default_request).await;
        let default_header = default_response
            .headers()
            .get(FLAGS_HEADER_NAME)
            .expect("flags header")
            .to_str()
            .expect("header str");
        assert_eq!(default_header, "global, route");
    }
}
