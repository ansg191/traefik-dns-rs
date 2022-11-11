use std::time::Duration;
use cloudflare::{
    endpoints::dns::{CreateDnsRecord, CreateDnsRecordParams, DeleteDnsRecord, DnsContent, DnsRecord, ListDnsRecords, ListDnsRecordsParams},
    framework::{
        auth::Credentials,
        async_api::{ApiClient, Client},
        Environment,
        HttpApiClientConfig,
        response::ApiFailure,
    },
};
use thiserror::Error;
use crate::rate_limit::RateLimit;

const DEFAULT_TTL: u32 = 300;
const DEFAULT_PROXIED: bool = false;
const REQUEST_LIMIT: u64 = 1200;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub struct CloudflareProvider {
    dest: String,
    zone_id: String,

    client: WrappedCloudflareClient,

    ttl: u32,
    proxied: bool,
}

impl CloudflareProvider {
    pub fn new(creds: Credentials, zone_id: String, dest: String) -> Result<Self, CloudflareError> {
        let client = Client::new(
            creds,
            HttpApiClientConfig::default(),
            Environment::Production,
        ).map_err(|e| match e.downcast::<reqwest::Error>() {
            Ok(e) => CloudflareError::NewClientError(e),
            Err(e) => panic!("Unexpected error: {}", e),
        })?;

        Ok(Self {
            dest,
            zone_id,
            client: WrappedCloudflareClient::new(client),
            ttl: DEFAULT_TTL,
            proxied: DEFAULT_PROXIED,
        })
    }

    pub fn ttl(&self) -> &u32 { &self.ttl }
    pub fn ttl_mut(&mut self) -> &mut u32 { &mut self.ttl }

    pub fn proxied(&self) -> &bool { &self.proxied }
    pub fn proxied_mut(&mut self) -> &mut bool { &mut self.proxied }
}

#[async_trait::async_trait]
impl super::Provider for CloudflareProvider {
    type Error = CloudflareError;

    fn destination(&self) -> &str { &self.dest }
    fn destination_mut(&mut self) -> &mut String { &mut self.dest }

    #[tracing::instrument(skip(self))]
    async fn list_records(&self) -> Result<Vec<String>, Self::Error> {
        let records = self.client.list_records(&self.zone_id, &self.dest).await?;
        Ok(records.into_iter().map(|r| r.name).collect())
    }

    #[tracing::instrument(skip(self))]
    async fn create_record(&self, host: &str) -> Result<(), Self::Error> {
        self.client.create_record(&self.zone_id, host, &self.dest, self.ttl, self.proxied).await
    }

    #[tracing::instrument(skip(self))]
    async fn delete_record(&self, host: &str) -> Result<(), Self::Error> {
        let record = self.client.list_records(&self.zone_id, &self.dest).await?
            .into_iter()
            .find(|r| r.name == host);

        if let Some(record) = record {
            self.client.delete_record(&self.zone_id, &record.id).await
        } else {
            Err(CloudflareError::RecordNotFound)
        }
    }
}

#[derive(Debug, Error)]
pub enum CloudflareError {
    #[error(transparent)]
    NewClientError(#[from] reqwest::Error),
    #[error(transparent)]
    ApiError(#[from] ApiFailure),
    #[error("record not found")]
    RecordNotFound,
}

struct WrappedCloudflareClient {
    client: Client,
    limiter: RateLimit,
}

impl WrappedCloudflareClient {
    fn new(client: Client) -> Self {
        Self {
            client,
            limiter: RateLimit::new(REQUEST_LIMIT, REQUEST_TIMEOUT),
        }
    }

    async fn list_records(&self, zone_id: &str, dest: &str) -> Result<Vec<DnsRecord>, CloudflareError> {
        self.limiter.ready().await;

        let request = ListDnsRecords {
            zone_identifier: zone_id,
            params: ListDnsRecordsParams {
                record_type: Some(DnsContent::CNAME { content: dest.to_string() }),
                name: None,
                page: None,
                per_page: Some(5000),
                order: None,
                direction: None,
                search_match: None,
            },
        };
        Ok(self.client.request(&request).await?.result)
    }

    async fn create_record(&self, zone_id: &str, host: &str, dest: &str, ttl: u32, proxied: bool) -> Result<(), CloudflareError> {
        self.limiter.ready().await;

        let request = CreateDnsRecord {
            zone_identifier: zone_id,
            params: CreateDnsRecordParams {
                ttl: Some(ttl),
                priority: None,
                proxied: Some(proxied),
                name: host,
                content: DnsContent::CNAME {
                    content: dest.to_string()
                },
            },
        };
        self.client.request(&request).await?;

        Ok(())
    }

    async fn delete_record(&self, zone_id: &str, record_id: &str) -> Result<(), CloudflareError> {
        self.limiter.ready().await;

        let request = DeleteDnsRecord {
            zone_identifier: zone_id,
            identifier: record_id,
        };
        self.client.request(&request).await?;

        Ok(())
    }
}
