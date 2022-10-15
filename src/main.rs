use std::collections::HashSet;
use std::fmt::{Debug, Display, Formatter};
use std::time::Duration;
use tokio::{
    time,
    time::MissedTickBehavior,
};
use crate::{
    dns::{
        Provider,
        route53::Route53Provider,
    },
    router::{
        Router,
        traefik::TraefikRouter,
    },
};

mod dns;
mod router;

const UPDATE_INTERVAL: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zone = std::env::var("ROUTE53_ZONE_ID")?;
    let traefik_url = std::env::var("TRAEFIK_API_URL")?;
    let cluster_domain = std::env::var("CLUSTER_DOMAIN")?;

    let cfg = aws_config::from_env().region("us-west-2").load().await;
    let d = Route53Provider::new(&cfg, zone, cluster_domain);
    let r = TraefikRouter::new(traefik_url)?;

    let mut interval = time::interval(UPDATE_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let timeout = time::timeout(UPDATE_INTERVAL, update_routes(&d, &r)).await;
        if let Ok(res) = timeout {
            if let Err(e) = res {
                eprintln!("update_routes error: {}", e);
            }
        } else {
            eprintln!("update_routes timed out");
        }
    }
}

async fn update_routes<D, R>(d: &D, r: &R) -> Result<(), UpdateRoutesError<D, R>>
    where
        D: Provider,
        R: Router {
    let routes: HashSet<_> = r.get_routes().await
        .map_err(UpdateRoutesError::<D, R>::RouterError)?
        .into_iter()
        .map(|r| r.host)
        .collect();

    // Add all active routes
    futures::future::try_join_all(
        routes.iter().map(|domain| {
            println!("Domain: {}", domain);
            d.create_record(domain)
        })
    ).await
        .map_err(UpdateRoutesError::<D, R>::ProviderError)?;

    let routes_to_delete: Vec<_> = d.list_records()
        .await
        .map_err(UpdateRoutesError::<D, R>::ProviderError)?
        .into_iter()
        .filter(|s| !routes.contains(s))
        .collect();

    println!("Routes to delete: {:?}", routes_to_delete);

    // Delete inactive routes
    futures::future::try_join_all(
        routes_to_delete.iter().map(|domain| d.delete_record(domain))
    ).await
        .map_err(UpdateRoutesError::<D, R>::ProviderError)?;

    Ok(())
}

#[derive(Debug)]
enum UpdateRoutesError<D: Provider, R: Router> {
    RouterError(R::Error),
    ProviderError(D::Error),
}

impl<D: Provider, R: Router> Display for UpdateRoutesError<D, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateRoutesError::RouterError(e) => Display::fmt(e, f),
            UpdateRoutesError::ProviderError(e) => Display::fmt(e, f)
        }
    }
}

impl<D: Provider + Debug, R: Router + Debug> std::error::Error for UpdateRoutesError<D, R> {}