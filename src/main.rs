#![allow(dead_code)]

use std::time::Duration;
use crate::{
    dns::route53::Route53Provider,
    router::traefik::TraefikRouter,
};

mod dns;
mod router;
mod updater;

const UPDATE_INTERVAL: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;

    let zone = std::env::var("ROUTE53_ZONE_ID")?;
    let traefik_url = std::env::var("TRAEFIK_API_URL")?;
    let cluster_domain = std::env::var("CLUSTER_DOMAIN")?;

    let cfg = aws_config::from_env().region("us-west-2").load().await;
    let client = aws_sdk_route53::Client::new(&cfg);

    let d = Route53Provider::new(client, zone, cluster_domain);
    let r = TraefikRouter::new(traefik_url)?;

    updater::Updater::new(d, r)
        .run(UPDATE_INTERVAL)
        .await?;

    Ok(())
}
