use std::{
    fmt::{Debug, Display, Formatter},
    collections::HashSet,
    time::Duration,
};
use tokio::{time, time::MissedTickBehavior};
use tokio::sync::Mutex;
use tracing::{error, info};
use crate::{
    dns::Provider,
    router::Router,
};

#[derive(Debug)]
pub struct Updater<D: Provider, R: Router> {
    provider: D,
    router: R,

    update_interval: Duration,

    current_routes: Mutex<HashSet<String>>,
}

impl<D: Provider, R: Router> Updater<D, R> {
    pub fn new(provider: D, router: R, update_interval: Duration) -> Self {
        Self {
            provider,
            router,
            update_interval,
            current_routes: Mutex::new(HashSet::new()),
        }
    }

    pub async fn run(&self) -> Result<(), UpdateRoutesError<D, R>> {
        let mut interval = time::interval(self.update_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            let timeout = time::timeout(self.update_interval, self.update_routes()).await;
            match timeout {
                Ok(res) => {
                    if let Err(e) = res {
                        error!("route updating returned an error error: {}", e);
                    }
                }
                Err(_) => {
                    error!("route updating timed out");
                }
            }
        }
    }

    #[tracing::instrument(skip(self), level = "info")]
    async fn update_routes(&self) -> Result<(), UpdateRoutesError<D, R>> {
        info!("updating routes");
        let mut current_routes = self.current_routes.lock().await;

        let routes: HashSet<_> = self.router.get_routes()
            .await
            .map_err(UpdateRoutesError::<D, R>::RouterError)?
            .into_iter()
            .map(|r| r.host)
            .collect();

        // Add all active routes
        futures::future::try_join_all(
            routes.iter()
                .filter(|&domain| !current_routes.contains(domain))
                .map(|domain| self.provider.create_record(domain))
        ).await
            .map_err(UpdateRoutesError::<D, R>::ProviderError)?;

        // Get routes to delete
        let routes_to_delete: Vec<_> = self.provider.list_records()
            .await
            .map_err(UpdateRoutesError::<D, R>::ProviderError)?
            .into_iter()
            .filter(|s| !routes.contains(s))
            .collect();

        if routes_to_delete.len() > 0 {
            info!(routes = ?routes_to_delete, "Deleting {} routes", routes_to_delete.len());
        }

        // Delete inactive routes
        futures::future::try_join_all(
            routes_to_delete.iter().map(|domain| self.provider.delete_record(domain))
        ).await
            .map_err(UpdateRoutesError::<D, R>::ProviderError)?;

        // Update current routes
        *current_routes = routes;

        Ok(())
    }
}

#[derive(Debug)]
pub enum UpdateRoutesError<D: Provider, R: Router> {
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