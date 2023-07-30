use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

static PROJECT_DIRS: Lazy<ProjectDirs> = Lazy::new(|| {
    ProjectDirs::from("com", "anshulg", "traefik-dns-rs")
        .expect("Unable to find project directories")
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
        let paths = [
            PROJECT_DIRS.config_dir(),
            Path::new("."),
            #[cfg(target_os = "linux")]
            Path::new("/etc/traefik-dns-rs"),
        ];
        for path in paths {
            let config_path = path.join("config.toml");
            debug!("Checking for config at {}", config_path.display());
            if config_path.exists() {
                debug!("Found config at {}", config_path.display());
                return Some(config_path);
            } else {
                debug!("No config found at {}", config_path.display());
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
