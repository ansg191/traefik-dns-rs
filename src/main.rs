#![allow(dead_code)]

use crate::{
    router::traefik::TraefikRouter,
    settings::{Provider, Settings},
    updater::Updater,
};
use std::{mem, time::Duration};

mod dns;
mod router;
mod settings;
mod updater;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;

    let cfg = Settings::new()?;

    run(cfg).await
}

async fn run(mut cfg: Settings) -> Result<(), Box<dyn std::error::Error>> {
    let router = TraefikRouter::new(mem::take(&mut cfg.traefik_url))?;

    let update_interval = parse_duration::parse(&cfg.update_interval)?;

    match cfg.provider {
        #[cfg(feature = "aws")]
        Provider::Route53(cfg) => run_route53(router, update_interval, cfg).await,
        #[cfg(feature = "cf")]
        Provider::Cloudflare(cfg) => run_cloudflare(router, update_interval, cfg).await,
    }
}

#[cfg(feature = "aws")]
async fn run_route53(
    router: TraefikRouter,
    update_interval: Duration,
    cfg: settings::Route53Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    let aws_cfg = aws_config::from_env().load().await;
    let client = aws_sdk_route53::Client::new(&aws_cfg);
    let mut provider = dns::route53::Route53Provider::new(client, cfg.zone_id, cfg.destination);

    if let Some(ttl) = cfg.ttl {
        *provider.ttl_mut() = ttl;
    }

    let updater = Updater::new(provider, router);

    Ok(updater.run(update_interval).await?)
}

#[cfg(feature = "cf")]
async fn run_cloudflare(
    router: TraefikRouter,
    update_interval: Duration,
    cfg: settings::CloudflareSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    let credentials = if let Some(token) = cfg.token {
        cloudflare::framework::auth::Credentials::UserAuthToken { token }
    } else if cfg.email.is_some() && cfg.api_key.is_some() {
        cloudflare::framework::auth::Credentials::UserAuthKey {
            email: cfg.email.unwrap(),
            key: cfg.api_key.unwrap(),
        }
    } else {
        panic!("missing cloudflare credentials");
    };

    let mut provider =
        dns::cloudflare::CloudflareProvider::new(credentials, cfg.zone_id, cfg.destination)?;

    if let Some(ttl) = cfg.ttl {
        *provider.ttl_mut() = ttl;
    }
    if let Some(proxied) = cfg.proxied {
        *provider.proxied_mut() = proxied;
    }

    let updater = Updater::new(provider, router);

    Ok(updater.run(update_interval).await?)
}
