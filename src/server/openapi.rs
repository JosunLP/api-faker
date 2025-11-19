use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::config::RouteConfig;

use super::{ERROR_HEADER_NAME, ERROR_QUERY_PARAM, FLAGS_HEADER_NAME, FLAGS_QUERY_PARAM};

pub(super) struct OpenApiBuilder {
    paths: Map<String, Value>,
}

impl OpenApiBuilder {
    pub fn new() -> Self {
        Self { paths: Map::new() }
    }

    pub fn record_route(&mut self, route: &RouteConfig) {
        let method_key = route.method.as_str().to_ascii_lowercase();
        let operation = Value::Object(Self::operation_for_route(route));
        let entry = self
            .paths
            .entry(route.path.clone())
            .or_insert_with(|| Value::Object(Map::new()));

        let path_obj = entry.as_object_mut().expect("Path entry must be an object");
        path_obj.insert(method_key, operation);
    }

    pub fn finish(self) -> Value {
        let mut root = Map::new();
        root.insert("openapi".into(), Value::String("3.1.0".into()));
        root.insert(
            "info".into(),
            json!({
                "title": "API Faker",
                "version": env!("CARGO_PKG_VERSION"),
                "description": "Automatically generated OpenAPI documentation based on mock_endpoints.json"
            }),
        );
        root.insert("servers".into(), json!([{ "url": "/" }]));
        root.insert("paths".into(), Value::Object(self.paths));
        Value::Object(root)
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
            "description": "Activates a configured error variant via query parameter",
            "schema": {
                "type": "string"
            }
        }),
        json!({
            "name": ERROR_HEADER_NAME,
            "in": "header",
            "required": false,
            "description": "Activates a configured error variant via HTTP header",
            "schema": {
                "type": "string"
            }
        }),
        json!({
            "name": FLAGS_QUERY_PARAM,
            "in": "query",
            "required": false,
            "description": "Activates variants based on configured flags via query parameter (repeat or comma-separate values)",
            "schema": {
                "type": "string"
            }
        }),
        json!({
            "name": FLAGS_HEADER_NAME,
            "in": "header",
            "required": false,
            "description": "Activates variants based on configured flags via HTTP header (comma-separated list)",
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
        .unwrap_or_else(|| format!("{} {} response", route.method, route.path));
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
    if !route.request_flags.is_empty() {
        vendor.insert("requestFlags".into(), json!(&route.request_flags));
    }
    if !route.response_flags.is_empty() {
        vendor.insert("responseFlags".into(), json!(&route.response_flags));
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
                    "requestFlags": &variant.request_flags,
                    "headers": &variant.headers,
                    "status": variant.status,
                    "delay_ms": variant.delay_ms,
                    "body": &variant.body,
                    "text_body": &variant.text_body,
                    "responseFlags": &variant.response_flags,
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
                    "requestFlags": &variant.request_flags,
                    "headers": &variant.headers,
                    "status": variant.status,
                    "delay_ms": variant.delay_ms,
                    "body": &variant.body,
                    "text_body": &variant.text_body,
                    "responseFlags": &variant.response_flags,
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
