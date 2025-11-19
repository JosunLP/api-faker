use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::Result;
use axum::http::Method;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

use crate::config::Config;
use crate::runtime::{RouteRuntime, VariantSource};

use super::openapi::OpenApiBuilder;

#[derive(Clone)]
pub struct AppState {
    pub(super) routes: Arc<HashMap<RouteKey, Vec<RouteRuntime>>>,
    pub(super) summaries: Arc<Vec<RouteSummary>>,
    pub(super) openapi: Arc<Value>,
    pub(super) default_response_flags: Arc<Vec<String>>,
}

impl TryFrom<Config> for AppState {
    type Error = anyhow::Error;

    fn try_from(config: Config) -> Result<Self> {
        let mut routes: HashMap<RouteKey, Vec<RouteRuntime>> = HashMap::new();
        let mut summaries = Vec::new();
        let mut openapi_builder = OpenApiBuilder::new();
        let global_response_flags = Arc::new(config.response_flags.clone());

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
                    let summary_request_flags = runtime.request_flags().map(|flags| flags.to_vec());
                    let summary_response_flags = runtime.response_flags().to_vec();
                    let entry = routes.entry(key.clone()).or_default();
                    if let Some(existing) = entry
                        .iter_mut()
                        .find(|existing| existing.same_query_signature(&runtime))
                    {
                        warn!(method = %method_string, path = %path_string, "Duplicate entry with identical query parameters overwritten");
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
                        request_flags: summary_request_flags,
                        response_flags: summary_response_flags,
                    });
                };

            let base_query_summary = if route.query.is_empty() {
                None
            } else {
                Some(route.query.clone())
            };
            let base_description = route.description.clone();
            let base_runtime = RouteRuntime::from_source(
                &route,
                VariantSource::Base,
                global_response_flags.as_ref(),
            )?;
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

                let runtime = RouteRuntime::from_source(
                    &route,
                    VariantSource::Query(variant),
                    global_response_flags.as_ref(),
                )?;
                push_runtime(runtime, variant_description, variant_query);
            }

            for error_variant in &route.error_variants {
                let description = error_variant
                    .description
                    .clone()
                    .or_else(|| route.description.clone());
                let runtime = RouteRuntime::from_source(
                    &route,
                    VariantSource::Error(error_variant),
                    global_response_flags.as_ref(),
                )?;
                push_runtime(runtime, description, None);
            }
        }

        let openapi = openapi_builder.finish();

        Ok(Self {
            routes: Arc::new(routes),
            summaries: Arc::new(summaries),
            openapi: Arc::new(openapi),
            default_response_flags: Arc::clone(&global_response_flags),
        })
    }
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub(super) struct RouteKey {
    pub(super) method: Method,
    pub(super) path: String,
}

#[derive(Clone, Serialize)]
pub(super) struct RouteSummary {
    method: String,
    path: String,
    status: u16,
    description: Option<String>,
    query: Option<BTreeMap<String, String>>,
    error_trigger: Option<String>,
    request_flags: Option<Vec<String>>,
    response_flags: Vec<String>,
}

#[derive(Clone)]
pub(super) struct RouteSummariesResponse(pub(super) Arc<Vec<RouteSummary>>);

impl Serialize for RouteSummariesResponse {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RouteConfig, RouteVariantConfig, ServerConfig};
    use axum::http::Method;
    use serde_json::json;
    use std::collections::BTreeSet;

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
            request_flags: BTreeSet::new(),
            response_flags: Vec::new(),
            delay_ms: None,
            description: Some("List users".into()),
            variants: Vec::new(),
            error_variants: Vec::new(),
        };

        let config = Config {
            server: ServerConfig::default(),
            response_flags: Vec::new(),
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
            request_flags: BTreeSet::new(),
            response_flags: Vec::new(),
            delay_ms: None,
            description: Some("List users".into()),
            variants: vec![RouteVariantConfig {
                query: variant_query,
                request_flags: BTreeSet::new(),
                headers: BTreeMap::new(),
                body: None,
                text_body: None,
                status: None,
                delay_ms: None,
                description: Some("Filtered".into()),
                response_flags: Vec::new(),
            }],
            error_variants: Vec::new(),
        };

        let config = Config {
            server: ServerConfig::default(),
            response_flags: Vec::new(),
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
            .any(|param| param["name"] == super::super::ERROR_QUERY_PARAM);
        assert!(has_error_query, "error query parameter missing");

        let has_error_header = params.iter().any(|param| {
            param["name"] == super::super::ERROR_HEADER_NAME && param["in"] == "header"
        });
        assert!(has_error_header, "error header parameter missing");
    }
}
