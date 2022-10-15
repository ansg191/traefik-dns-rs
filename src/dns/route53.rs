use aws_config::SdkConfig;
use aws_sdk_route53::Client;
use aws_sdk_route53::error::{ChangeResourceRecordSetsError, ListResourceRecordSetsError};
use aws_sdk_route53::model::{Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType};
use aws_sdk_route53::types::SdkError;
use thiserror::Error;
use super::Provider;

const DEFAULT_TTL: i64 = 300;

#[derive(Debug, Clone)]
pub struct Route53Provider {
    dest: String,
    hosted_zone_id: String,
    client: Client,

    ttl: i64,
}

impl Route53Provider {
    pub fn new(config: &SdkConfig, hosted_zone_id: String, dest: String) -> Self {
        Self {
            dest,
            hosted_zone_id,
            client: Client::new(config),
            ttl: DEFAULT_TTL,
        }
    }

    pub fn ttl(&self) -> &i64 {
        &self.ttl
    }
    pub fn ttl_mut(&mut self) -> &mut i64 {
        &mut self.ttl
    }

    fn change_batch(&self, action: ChangeAction, host: &str, ttl: Option<i64>) -> ChangeBatch {
        ChangeBatch::builder()
            .changes(Change::builder()
                .action(action)
                .resource_record_set(ResourceRecordSet::builder()
                    .name(host)
                    .r#type(RrType::Cname)
                    .resource_records(ResourceRecord::builder()
                        .value(self.dest.clone())
                        .build())
                    .ttl(ttl.unwrap_or(self.ttl))
                    .build())
                .build()
            )
            .build()
    }
}

#[async_trait::async_trait]
impl Provider for Route53Provider {
    type Error = Route53Error;

    fn destination(&self) -> &str {
        &self.dest
    }

    fn destination_mut(&mut self) -> &mut String {
        &mut self.dest
    }

    async fn list_records(&self) -> Result<Vec<String>, Self::Error> {
        Ok(self.client.list_resource_record_sets()
            .hosted_zone_id(self.hosted_zone_id.clone())
            .send()
            .await?
            .resource_record_sets()
            .unwrap_or_default()
            .iter()
            .filter(|r| {
                let dest = r.resource_records()
                    .unwrap_or_default()
                    .iter()
                    .find(|v| v.value() == Some(&self.dest));
                r.r#type() == Some(&RrType::Cname) && dest.is_some()
            })
            .filter_map(|r| r.name().map(ToOwned::to_owned))
            .map(|mut s| {
                // Remove last dot
                s.pop();
                s
            })
            .collect())
    }

    async fn create_record(&self, host: &str) -> Result<(), Self::Error> {
        self.client.change_resource_record_sets()
            .hosted_zone_id(self.hosted_zone_id.clone())
            .change_batch(self.change_batch(ChangeAction::Upsert, host, None))
            .send()
            .await?;

        Ok(())
    }

    async fn delete_record(&self, host: &str) -> Result<(), Self::Error> {
        let records = self.client.list_resource_record_sets()
            .hosted_zone_id(self.hosted_zone_id.clone())
            .send()
            .await?;
        let record = records
            .resource_record_sets()
            .unwrap_or_default()
            .iter()
            .find(|r| {
                // Remove trailing dot
                let name = r.name().map(|d| &d[..d.len() - 1]);
                name == Some(host) && r.r#type() == Some(&RrType::Cname)
            })
            .ok_or(Route53Error::MissingRecord)?;

        self.client.change_resource_record_sets()
            .hosted_zone_id(self.hosted_zone_id.clone())
            .change_batch(self.change_batch(ChangeAction::Delete, host, record.ttl()))
            .send()
            .await?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Route53Error {
    #[error(transparent)]
    ChangeSetsError(#[from] SdkError<ChangeResourceRecordSetsError>),
    #[error(transparent)]
    ListSetsError(#[from] SdkError<ListResourceRecordSetsError>),
    #[error("missing record")]
    MissingRecord,
}