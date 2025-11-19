use axum::Router;
use axum::routing::get;
use tower_http::cors::CorsLayer;

use super::AppState;
use super::handlers::{
    dispatch, favicon, health, list_routes, openapi_spec, robots_txt, swagger_ui,
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/__health", get(health))
        .route("/__routes", get(list_routes))
        .route("/__openapi.json", get(openapi_spec))
        .route("/__swagger", get(swagger_ui))
        .route("/robots.txt", get(robots_txt))
        .route("/favicon.ico", get(favicon))
        .fallback(dispatch)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
