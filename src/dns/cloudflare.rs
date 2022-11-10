use cloudflare::endpoints::dns::{CreateDnsRecord, CreateDnsRecordParams, DeleteDnsRecord, DnsContent, DnsRecord, ListDnsRecords, ListDnsRecordsParams};
use cloudflare::framework::async_api::{ApiClient, Client};
use cloudflare::framework::auth::Credentials;
use cloudflare::framework::{Environment, HttpApiClientConfig};
use cloudflare::framework::response::ApiFailure;
use thiserror::Error;

const DEFAULT_TTL: u32 = 300;
const DEFAULT_PROXIED: bool = false;

pub struct CloudflareProvider {
    dest: String,
    zone_id: String,

    client: Client,

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
            client,
            ttl: DEFAULT_TTL,
            proxied: DEFAULT_PROXIED,
        })
    }

    pub fn ttl(&self) -> &u32 { &self.ttl }
    pub fn ttl_mut(&mut self) -> &mut u32 { &mut self.ttl }

    pub fn proxied(&self) -> &bool { &self.proxied }
    pub fn proxied_mut(&mut self) -> &mut bool { &mut self.proxied }

    async fn list_records(&self) -> Result<Vec<DnsRecord>, CloudflareError> {
        let request = ListDnsRecords {
            zone_identifier: &self.zone_id,
            params: ListDnsRecordsParams {
                record_type: Some(DnsContent::CNAME { content: self.dest.clone() }),
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
}

#[async_trait::async_trait]
impl super::Provider for CloudflareProvider {
    type Error = CloudflareError;

    fn destination(&self) -> &str { &self.dest }
    fn destination_mut(&mut self) -> &mut String { &mut self.dest }

    #[tracing::instrument(skip(self))]
    async fn list_records(&self) -> Result<Vec<String>, Self::Error> {
        let records = self.list_records().await?;
        Ok(records.into_iter().map(|r| r.name).collect())
    }

    #[tracing::instrument(skip(self))]
    async fn create_record(&self, host: &str) -> Result<(), Self::Error> {
        let request = CreateDnsRecord {
            zone_identifier: &self.zone_id,
            params: CreateDnsRecordParams {
                ttl: Some(self.ttl),
                priority: None,
                proxied: Some(self.proxied),
                name: host,
                content: DnsContent::CNAME {
                    content: self.dest.clone()
                },
            },
        };
        self.client.request(&request).await?;

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn delete_record(&self, host: &str) -> Result<(), Self::Error> {
        let record = self.list_records().await?
            .into_iter()
            .find(|r| r.name == host);

        if let Some(record) = record {
            let request = DeleteDnsRecord {
                zone_identifier: &self.zone_id,
                identifier: &record.id,
            };
            self.client.request(&request).await?;

            Ok(())
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