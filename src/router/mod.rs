pub mod traefik;

#[async_trait::async_trait]
pub trait Router {
    type Error: std::error::Error;

    async fn get_routes(&self) -> Result<Vec<Route>, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Route {
    pub id: String,
    pub host: String,
}