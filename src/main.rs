// Allow deprecated items to silence warnings from build_info.
// TODO: Remove this when the build_info crate is updated.
#![allow(dead_code, deprecated)]

use crate::{router::traefik::TraefikRouter, settings::Settings};
use std::{mem, time::Duration};
use tracing::info;

mod dns;
mod router;
mod settings;
mod updater;

build_info::build_info!(fn build_info);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = get_subscriber();
    tracing::subscriber::set_global_default(subscriber)?;

    log_app_info();

    let cfg = Settings::new()?;

    run(cfg).await
}

#[cfg(debug_assertions)]
fn get_subscriber() -> impl tracing::Subscriber + Send + Sync + 'static {
    tracing_subscriber::FmtSubscriber::builder()
        .with_thread_names(true)
        .with_thread_ids(true)
        .with_level(true)
        .with_target(true)
        .with_ansi(true)
        .with_file(true)
        .with_line_number(true)
        .finish()
}

#[cfg(not(debug_assertions))]
fn get_subscriber() -> impl tracing::Subscriber + Send + Sync + 'static {
    tracing_subscriber::FmtSubscriber::builder()
        .with_thread_names(false)
        .with_thread_ids(false)
        .with_level(true)
        .with_target(false)
        .with_ansi(false)
        .with_file(false)
        .with_line_number(false)
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new())
        .finish()
}

fn log_app_info() {
    let info = build_info();

    if let Some(build_info::VersionControl::Git(vc)) = &info.version_control {
        info!(
            version=%info.crate_info.version,
            enabled_features=?info.crate_info.enabled_features,
            rustc_version=%info.compiler.version,
            profile=%info.profile,
            git.commit_id=%vc.commit_short_id,
            git.dirty=%vc.dirty,
            git.branch=?vc.branch,
            git.tags=?vc.tags,
            "Starting up {}",
            info.crate_info.name
        );
    } else {
        info!(
            version=%info.crate_info.version,
            enabled_features=?info.crate_info.enabled_features,
            rustc_version=%info.compiler.version,
            profile=%info.profile,
            "Starting up {}",
            info.crate_info.name
        );
    }
}

#[cfg_attr(not(any(feature = "cf", feature = "aws")), allow(unused_variables))]
async fn run(mut cfg: Settings) -> Result<(), Box<dyn std::error::Error>> {
    let router = TraefikRouter::new(mem::take(&mut cfg.traefik_url))?;

    let update_interval: Duration = cfg.update_interval.parse::<humantime::Duration>()?.into();

    match cfg.provider {
        #[cfg(feature = "aws")]
        Some(settings::Provider::Route53(cfg)) => run_route53(router, update_interval, cfg).await,
        #[cfg(feature = "cf")]
        Some(settings::Provider::Cloudflare(cfg)) => {
            run_cloudflare(router, update_interval, cfg).await
        }
        #[cfg(not(any(feature = "cf", feature = "aws")))]
        Some(_) => panic!("Unsupported provider"),
        None => Err("No provider configured")?,
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

    let updater = updater::Updater::new(provider, router);

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

    let updater = updater::Updater::new(provider, router);

    Ok(updater.run(update_interval).await?)
}
