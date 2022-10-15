#[cfg(feature = "aws")]
pub mod route53;
#[cfg(feature = "cloudflare")]
pub mod cloudflare;

#[async_trait::async_trait]
pub trait Provider: Send {
    type Error: std::error::Error + Send;

    fn destination(&self) -> &str;
    fn destination_mut(&mut self) -> &mut String;

    async fn list_records(&self) -> Result<Vec<String>, Self::Error>;
    async fn create_record(&self, host: &str) -> Result<(), Self::Error>;
    async fn delete_record(&self, host: &str) -> Result<(), Self::Error>;
}