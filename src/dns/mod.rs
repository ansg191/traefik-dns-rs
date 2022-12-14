#[cfg(feature = "cloudflare")]
pub mod cloudflare;
#[cfg(feature = "aws")]
pub mod route53;

#[cfg_attr(test, mockall::automock(type Error = tests::MockProviderError;))]
#[async_trait::async_trait]
pub trait Provider: Send {
    type Error: std::error::Error + Send;

    fn destination(&self) -> &str;
    fn destination_mut(&mut self) -> &mut String;

    async fn list_records(&self) -> Result<Vec<String>, Self::Error>;
    async fn create_record(&self, host: &str) -> Result<(), Self::Error>;
    async fn delete_record(&self, host: &str) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    /// Mock error type for testing
    #[derive(Debug)]
    pub struct MockProviderError;

    impl std::fmt::Display for MockProviderError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MockProviderError")
        }
    }

    impl std::error::Error for MockProviderError {}
}
