use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Client, IntoUrl, Url};
use serde::Deserialize;
use thiserror::Error;
use tracing::debug;
use crate::router::Route;

static HOST_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("Host\\(`(.+?)`\\)").unwrap());

#[derive(Debug)]
pub struct TraefikRouter {
    base_url: Url,
    client: Client,
}

impl TraefikRouter {
    pub fn new<U: IntoUrl>(url: U) -> Result<Self, TraefikError> {
        let base_url = url.into_url()?;

        if base_url.cannot_be_a_base() {
            Err(TraefikError::BadBaseUrl)
        } else {
            Ok(Self {
                base_url,
                client: Client::new(),
            })
        }
    }
}

#[async_trait::async_trait]
impl super::Router for TraefikRouter {
    type Error = TraefikError;

    #[tracing::instrument(skip(self))]
    async fn get_routes(&self) -> Result<Vec<Route>, Self::Error> {
        let url = self.base_url.join("api/http/routers")?;
        let routes = self.client.get(url)
            .send()
            .await?
            .json::<Vec<TraefikRoute>>()
            .await?;

        debug!(?routes, "got {} routes from Traefik", routes.len());

        Ok(routes.iter()
            .flat_map(|r| parse_domains(&r.rule)
                .map(|d| Route {
                    id: r.name.clone(),
                    host: d.to_owned(),
                })
            )
            .collect()
        )
    }
}

/// Parses domains out of Traefik Rule expressions.
fn parse_domains(rule: &str) -> impl Iterator<Item=&str> {
    HOST_REGEX.captures_iter(rule)
        .filter_map(|cap| cap.get(1))
        .map(|m| m.as_str())
}

#[derive(Debug, Error)]
pub enum TraefikError {
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error("bad base url")]
    BadBaseUrl,
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
}

#[derive(Debug, Deserialize)]
struct TraefikRoute {
    rule: String,
    name: String,
}