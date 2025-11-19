use std::collections::{BTreeMap, BTreeSet};

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::response::Response;
use tracing::warn;
use url::form_urlencoded;

use super::{ERROR_HEADER_NAME, ERROR_QUERY_PARAM, FLAGS_HEADER_NAME, FLAGS_QUERY_PARAM};

pub(super) fn extract_error_trigger(
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

pub(super) fn extract_request_flags(
    headers: &HeaderMap,
    query: Option<&BTreeMap<String, Vec<String>>>,
) -> Option<BTreeSet<String>> {
    let mut flags = BTreeSet::new();

    if let Some(value) = headers
        .get(FLAGS_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
    {
        extend_flags(&mut flags, value);
    }

    if let Some(query) = query
        && let Some(values) = query.get(FLAGS_QUERY_PARAM)
    {
        for value in values {
            extend_flags(&mut flags, value);
        }
    }

    if flags.is_empty() { None } else { Some(flags) }
}

pub(super) fn attach_response_flags(response: &mut Response, flags: &[String]) {
    if flags.is_empty() {
        return;
    }

    let joined = flags.join(", ");
    match HeaderValue::from_str(&joined) {
        Ok(value) => {
            response
                .headers_mut()
                .insert(HeaderName::from_static(FLAGS_HEADER_NAME), value);
        }
        Err(error) => {
            warn!(?error, "Failed to set response flags");
        }
    }
}

pub(super) fn parse_query_map(raw: &str) -> BTreeMap<String, Vec<String>> {
    form_urlencoded::parse(raw.as_bytes()).into_owned().fold(
        BTreeMap::new(),
        |mut acc, (key, value)| {
            acc.entry(key).or_default().push(value);
            acc
        },
    )
}

fn extend_flags(target: &mut BTreeSet<String>, raw: &str) {
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        target.insert(trimmed.to_string());
    }
}
