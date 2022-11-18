pub mod traefik;

#[cfg_attr(test, mockall::automock(type Error = tests::MockRouterError;))]
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

#[cfg(test)]
mod tests {
    /// Mock error type for testing
    #[derive(Debug)]
    pub struct MockRouterError;

    impl std::fmt::Display for MockRouterError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MockRouterError")
        }
    }

    impl std::error::Error for MockRouterError {}
}
