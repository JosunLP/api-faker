use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{Context, Result};
use axum::http::Method;
use serde::Deserialize;
use tokio::fs;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub response_flags: Vec<String>,
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ServerConfig {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

impl Config {
    pub async fn from_file(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read '{}'", path.display()))?;
        let config = serde_json::from_str(&data)
            .with_context(|| format!("Invalid JSON in '{}'", path.display()))?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
pub struct RouteConfig {
    #[serde(deserialize_with = "method_from_str")]
    pub method: Method,
    pub path: String,
    #[serde(default = "default_status")]
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub text_body: Option<String>,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default)]
    pub request_flags: BTreeSet<String>,
    #[serde(default)]
    pub response_flags: Vec<String>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub variants: Vec<RouteVariantConfig>,
    #[serde(default)]
    pub error_variants: Vec<RouteErrorVariantConfig>,
}

#[derive(Debug, Deserialize)]
pub struct RouteVariantConfig {
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default)]
    pub request_flags: BTreeSet<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub text_body: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub response_flags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RouteErrorVariantConfig {
    pub name: String,
    #[serde(default)]
    pub request_flags: BTreeSet<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub text_body: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub response_flags: Vec<String>,
}

fn default_status() -> u16 {
    200
}

fn method_from_str<'de, D>(deserializer: D) -> Result<Method, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    raw.parse::<Method>().map_err(serde::de::Error::custom)
}
