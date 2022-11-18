#![allow(dead_code)]

use crate::{
    router::{traefik::TraefikRouter},
    settings::{Provider, Settings},
    updater::Updater,
};
use std::mem;

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

    match cfg.provider {
        #[cfg(feature = "aws")]
        Provider::Route53(_) => run_route53(router, cfg).await,
        #[cfg(feature = "cf")]
        Provider::Cloudflare(_) => run_cloudflare(router, cfg).await,
    }
}

#[cfg(feature = "aws")]
async fn run_route53(
    router: TraefikRouter,
    cfg: Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    let aws_cfg = aws_config::from_env().load().await;
    let client = aws_sdk_route53::Client::new(&aws_cfg);
    let mut provider = dns::route53::Route53Provider::new(client, cfg.zone_id, cfg.destination);

    if let Provider::Route53(cfg) = cfg.provider {
        if let Some(ttl) = cfg.ttl {
            *provider.ttl_mut() = ttl;
        }
    } else {
        unreachable!()
    }

    let updater = Updater::new(provider, router);

    let update_interval = parse_duration::parse(&cfg.update_interval)?;
    Ok(updater.run(update_interval).await?)
}

#[cfg(feature = "cf")]
async fn run_cloudflare(
    router: TraefikRouter,
    mut cfg: Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    let credentials = if let Provider::Cloudflare(cfg) = &mut cfg.provider {
        if let Some(token) = &mut cfg.token {
            cloudflare::framework::auth::Credentials::UserAuthToken {
                token: mem::take(token),
            }
        } else if cfg.email.is_some() && cfg.api_key.is_some() {
            cloudflare::framework::auth::Credentials::UserAuthKey {
                email: cfg.email.take().unwrap(),
                key: cfg.api_key.take().unwrap(),
            }
        } else {
            panic!("missing cloudflare credentials");
        }
    } else {
        unreachable!()
    };

    let mut provider =
        dns::cloudflare::CloudflareProvider::new(credentials, cfg.zone_id, cfg.destination)?;

    if let Provider::Cloudflare(cfg) = cfg.provider {
        if let Some(ttl) = cfg.ttl {
            *provider.ttl_mut() = ttl;
        }
        if let Some(proxied) = cfg.proxied {
            *provider.proxied_mut() = proxied;
        }
    } else {
        unreachable!()
    }

    let updater = Updater::new(provider, router);

    let update_interval = parse_duration::parse(&cfg.update_interval)?;
    Ok(updater.run(update_interval).await?)
}
