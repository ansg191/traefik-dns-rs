use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

#[cfg(target_os = "linux")]
static DEFAULT_SEARCH_PATHS: Lazy<[PathBuf; 3]> = Lazy::new(|| {
    [
        PathBuf::from("."),
        PathBuf::from(
            shellexpand::tilde(build_info::format!("~/.config/{}", $.crate_info.name)).into_owned(),
        ),
        PathBuf::from(build_info::format!("/etc/{}", $.crate_info.name)),
    ]
});

#[cfg(target_os = "macos")]
static DEFAULT_SEARCH_PATHS: Lazy<[PathBuf; 3]> = Lazy::new(|| {
    [
        PathBuf::from("."),
        PathBuf::from(
            shellexpand::tilde(build_info::format!("~/.config/{}", $.crate_info.name)).into_owned(),
        ),
        PathBuf::from(
            shellexpand::tilde(
                build_info::format!("~/Library/Application Support/{}", $.crate_info.name),
            )
            .into_owned(),
        ),
    ]
});

#[cfg(target_os = "windows")]
static DEFAULT_SEARCH_PATHS: Lazy<[PathBuf; 3]> = Lazy::new(|| {
    [
        PathBuf::from("."),
        PathBuf::from(
            shellexpand::env(build_info::format!("%APPDATA%/{}", $.crate_info.name)).into_owned(),
        ),
    ]
});

#[cfg(feature = "aws")]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Route53Settings {
    pub zone_id: String,
    pub destination: String,

    pub ttl: Option<i64>,
}

#[cfg(feature = "cf")]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CloudflareSettings {
    pub zone_id: String,
    pub destination: String,

    pub token: Option<String>,
    pub email: Option<String>,
    pub api_key: Option<String>,

    pub ttl: Option<u32>,
    pub proxied: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Provider {
    #[cfg(feature = "aws")]
    Route53(Route53Settings),
    #[cfg(feature = "cf")]
    Cloudflare(CloudflareSettings),
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub traefik_url: String,
    pub update_interval: String,
    pub provider: Option<Provider>,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let path = Self::find_config().ok_or(ConfigError::NoConfigFound)?;

        info!("Loading settings from {}", path.display());

        let contents = std::fs::read_to_string(&path)?;

        Ok(toml::from_str(&contents)?)
    }

    fn find_config() -> Option<PathBuf> {
        for path in &*DEFAULT_SEARCH_PATHS {
            let config_path = path.join("config.toml");
            debug!("Checking for config at {}", config_path.display());
            if config_path.exists() {
                return Some(config_path);
            }
        }
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("No config file found")]
    NoConfigFound,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    TomlError(#[from] toml::de::Error),
}
