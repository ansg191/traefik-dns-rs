use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Client, IntoUrl, Url};
use serde::Deserialize;
use thiserror::Error;
use tracing::debug;

use crate::router::Route;

// https://regex101.com/r/eTXvjo/1
static HOST_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("Host\\((.+?)\\)").unwrap());
// https://regex101.com/r/MZWk3s/1
static HOST_ARG_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("`(.+?)`").unwrap());

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
        let routes = self
            .client
            .get(url)
            .send()
            .await?
            .json::<Vec<TraefikRoute>>()
            .await?;

        debug!(?routes, "got {} routes from Traefik", routes.len());

        Ok(routes
            .iter()
            .flat_map(|r| {
                parse_domains(&r.rule).map(|d| Route {
                    id: r.name.clone(),
                    host: d.to_owned(),
                })
            })
            .collect())
    }
}

/// Parses domains out of Traefik Rule expressions.
fn parse_domains(rule: &str) -> impl Iterator<Item = &str> {
    HOST_REGEX
        .captures_iter(rule)
        .filter_map(|cap| cap.get(1))
        .flat_map(|m| HOST_ARG_REGEX.captures_iter(m.as_str()))
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

#[cfg(test)]
mod tests {
    use httptest::{matchers::*, responders::*, Expectation, Server};

    use super::*;
    use crate::router::Router;

    #[test]
    fn test_parse_domains() {
        let domains: Vec<&str> = parse_domains("Host(`example.com`)").collect();
        assert_eq!(domains, vec!["example.com"]);

        let domains: Vec<&str> = parse_domains("Host(`example1.com`, `example2.org`)").collect();
        assert_eq!(domains, vec!["example1.com", "example2.org"]);

        let domains: Vec<&str> =
            parse_domains("Host(`example1.com`), Host(`example2.org`)").collect();
        assert_eq!(domains, vec!["example1.com", "example2.org"]);

        let domains: Vec<&str> =
            parse_domains("Host(`example1.com`) || Host(`example2.org`) && Path(`/foo`)").collect();
        assert_eq!(domains, vec!["example1.com", "example2.org"]);

        let domains: Vec<&str> = parse_domains("HostSNI(*)").collect();
        assert_eq!(domains, Vec::<&str>::new());
    }

    #[tokio::test]
    async fn test_get_routes() {
        let server = Server::run();
        let base_url = server.url_str("/");

        server.expect(
            Expectation::matching(request::method_path("GET", "/api/http/routers")).respond_with(
                status_code(200).body(
                    r#"
                    [
                        {
                            "rule": "Host(`example1.com`)",
                            "name": "example1"
                        },
                        {
                            "rule": "Host(`example2.org`)",
                            "name": "example2"
                        },
                        {
                            "rule": "Host(`example3.net`, `example4.net`)",
                            "name": "example3"
                        },
                        {
                            "rule": "Path(`/foo`)",
                            "name": "path"
                        }
                    ]
                    "#,
                ),
            ),
        );

        let router = TraefikRouter::new(base_url).unwrap();

        let routes = router.get_routes().await.unwrap();
        assert_eq!(
            routes,
            vec![
                Route {
                    id: "example1".to_owned(),
                    host: "example1.com".to_owned()
                },
                Route {
                    id: "example2".to_owned(),
                    host: "example2.org".to_owned()
                },
                Route {
                    id: "example3".to_owned(),
                    host: "example3.net".to_owned()
                },
                Route {
                    id: "example3".to_owned(),
                    host: "example4.net".to_owned()
                },
            ]
        );
    }
}
