use axum::Router;
use axum::routing::get;
use tower_http::cors::CorsLayer;

use super::AppState;
use super::handlers::{dispatch, health, list_routes, openapi_spec, swagger_ui};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/__health", get(health))
        .route("/__routes", get(list_routes))
        .route("/__openapi.json", get(openapi_spec))
        .route("/__swagger", get(swagger_ui))
        .fallback(dispatch)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
