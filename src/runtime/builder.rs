use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use axum::body::Bytes;
use axum::http::{HeaderName, HeaderValue, StatusCode, header::CONTENT_TYPE};

use crate::config::{RouteConfig, RouteErrorVariantConfig, RouteVariantConfig};

use super::{
    flags::{flags_arc, merge_response_flags},
    query::map_query_arc,
};

#[derive(Clone)]
pub struct RouteRuntime {
    pub(crate) status: StatusCode,
    pub(crate) headers: Arc<Vec<(HeaderName, HeaderValue)>>,
    pub(crate) body: Option<Bytes>,
    pub(crate) delay: Option<Duration>,
    pub(crate) query_params: Option<Arc<Vec<(String, String)>>>,
    pub(crate) error_trigger: Option<Arc<String>>,
    pub(crate) request_flags: Option<Arc<Vec<String>>>,
    pub(crate) response_flags: Arc<Vec<String>>,
}

#[derive(Clone, Copy)]
pub enum VariantSource<'a> {
    Base,
    Query(&'a RouteVariantConfig),
    Error(&'a RouteErrorVariantConfig),
}

impl RouteRuntime {
    pub fn from_source(
        route: &RouteConfig,
        source: VariantSource<'_>,
        global_response_flags: &[String],
    ) -> Result<Self> {
        let status_raw = match source {
            VariantSource::Base => route.status,
            VariantSource::Query(variant) => variant.status.unwrap_or(route.status),
            VariantSource::Error(variant) => variant.status.unwrap_or(route.status),
        };

        let status = StatusCode::from_u16(status_raw)
            .with_context(|| format!("Invalid status code {status_raw}"))?;

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
                .with_context(|| format!("Invalid header name: {name}"))?;
            let header_value: HeaderValue = value
                .parse()
                .with_context(|| format!("Invalid header value for {name}"))?;

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
                        "Route {} {} defines both 'body' and 'text_body'. Please only set one of them.",
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
                        "Variant of {} {} defines both 'body' and 'text_body'. Please only set one of them.",
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
                        "Error variant '{}' for {} {} defines both 'body' and 'text_body'. Please only set one of them.",
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
                let bytes =
                    serde_json::to_vec(body).context("Response body could not be serialized")?;
                (Some(Bytes::from(bytes)), Some("application/json"))
            }
            BodySpec::Text(text) => {
                let bytes = Bytes::from(text.to_owned());
                (Some(bytes), Some("text/plain; charset=utf-8"))
            }
            BodySpec::None => (None, None),
        };

        if body_bytes.is_some()
            && !has_content_type
            && let Some(default) = default_ct
        {
            headers.push((CONTENT_TYPE, HeaderValue::from_static(default)));
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
                        "Error variant for {} {} requires a non-empty 'name' field.",
                        route.method,
                        route.path
                    );
                }
                Some(Arc::new(trimmed.to_string()))
            }
            _ => None,
        };

        let request_flags = match source {
            VariantSource::Base => flags_arc(&route.request_flags),
            VariantSource::Query(variant) => {
                if variant.request_flags.is_empty() {
                    flags_arc(&route.request_flags)
                } else {
                    flags_arc(&variant.request_flags)
                }
            }
            VariantSource::Error(variant) => {
                if variant.request_flags.is_empty() {
                    flags_arc(&route.request_flags)
                } else {
                    flags_arc(&variant.request_flags)
                }
            }
        };

        let response_flags = merge_response_flags(
            global_response_flags,
            &route.response_flags,
            match source {
                VariantSource::Base => None,
                VariantSource::Query(variant) => Some(&variant.response_flags),
                VariantSource::Error(variant) => Some(&variant.response_flags),
            },
        );

        Ok(Self {
            status,
            headers: Arc::new(headers),
            body: body_bytes,
            delay,
            query_params,
            error_trigger,
            request_flags,
            response_flags: Arc::new(response_flags),
        })
    }

    pub fn error_trigger(&self) -> Option<&str> {
        self.error_trigger.as_deref().map(|s| s.as_str())
    }

    pub fn is_error(&self) -> bool {
        self.error_trigger.is_some()
    }

    pub fn headers(&self) -> &[(HeaderName, HeaderValue)] {
        self.headers.as_slice()
    }

    pub fn request_flags(&self) -> Option<&[String]> {
        self.request_flags.as_ref().map(|arc| arc.as_slice())
    }

    pub fn response_flags(&self) -> &[String] {
        self.response_flags.as_slice()
    }

    pub fn body(&self) -> Option<Bytes> {
        self.body.as_ref().cloned()
    }

    pub fn delay(&self) -> Option<Duration> {
        self.delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};

    fn base_route() -> RouteConfig {
        RouteConfig {
            method: Method::GET,
            path: "/users".into(),
            status: 200,
            headers: BTreeMap::new(),
            body: Some(json!({"foo": "bar"})),
            text_body: None,
            query: BTreeMap::new(),
            request_flags: BTreeSet::new(),
            response_flags: Vec::new(),
            delay_ms: None,
            description: None,
            variants: Vec::new(),
            error_variants: Vec::new(),
        }
    }

    #[test]
    fn variant_text_body_overrides_base_json() {
        let route = base_route();
        let variant = RouteVariantConfig {
            query: BTreeMap::new(),
            request_flags: BTreeSet::new(),
            headers: BTreeMap::new(),
            body: None,
            text_body: Some("hello world".into()),
            status: None,
            delay_ms: None,
            description: None,
            response_flags: Vec::new(),
        };

        let runtime = RouteRuntime::from_source(&route, VariantSource::Query(&variant), &[])
            .expect("runtime");
        let body = runtime.body().expect("body");
        assert_eq!(body, Bytes::from_static(b"hello world"));

        assert!(
            runtime
                .headers()
                .iter()
                .any(|(name, value)| name == CONTENT_TYPE && value == "text/plain; charset=utf-8")
        );
    }

    #[test]
    fn error_variant_inherits_base_headers() {
        let mut route = base_route();
        route.headers.insert("x-base".into(), "foo".into());

        let error_variant = RouteErrorVariantConfig {
            name: "boom".into(),
            request_flags: BTreeSet::new(),
            headers: BTreeMap::new(),
            body: None,
            text_body: Some("broken".into()),
            status: Some(503),
            delay_ms: None,
            description: None,
            response_flags: Vec::new(),
        };

        let runtime = RouteRuntime::from_source(&route, VariantSource::Error(&error_variant), &[])
            .expect("runtime");
        assert_eq!(runtime.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(runtime.error_trigger(), Some("boom"));
        assert!(
            runtime
                .headers()
                .iter()
                .any(|(name, value)| name == CONTENT_TYPE && value == "text/plain; charset=utf-8")
        );
    }
}
