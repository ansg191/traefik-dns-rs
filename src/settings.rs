use config::{Config, ConfigError, File};
use serde::Deserialize;

#[cfg(feature = "aws")]
#[derive(Debug, Deserialize)]
pub struct Route53Settings {
    pub ttl: Option<i64>,
}

#[cfg(feature = "cf")]
#[derive(Debug, Deserialize)]
pub struct CloudflareSettings {
    pub token: Option<String>,
    pub email: Option<String>,
    pub api_key: Option<String>,

    pub ttl: Option<u32>,
    pub proxied: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Provider {
    #[cfg(feature = "aws")]
    Route53(Route53Settings),
    #[cfg(feature = "cf")]
    Cloudflare(CloudflareSettings),
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub zone_id: String,
    pub traefik_url: String,
    pub destination: String,
    pub update_interval: String,
    pub provider: Provider,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let cfg = Config::builder()
            .add_source(File::with_name("config").required(false))
            .add_source(File::with_name("/etc/traefik-dns-rs/config").required(false))
            .add_source(config::Environment::with_prefix("APP"))
            .build()?;

        cfg.try_deserialize()
    }
}
