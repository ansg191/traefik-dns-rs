use super::Provider;
use aws_sdk_route53::{
    error::{ChangeResourceRecordSetsError, ListResourceRecordSetsError},
    model::{Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType},
    types::SdkError,
    Client,
};
use thiserror::Error;

const DEFAULT_TTL: i64 = 300;

#[derive(Debug, Clone)]
pub struct Route53Provider {
    dest: String,
    hosted_zone_id: String,
    client: Client,

    ttl: i64,
}

impl Route53Provider {
    pub fn new(client: Client, hosted_zone_id: String, dest: String) -> Self {
        Self {
            dest,
            hosted_zone_id,
            client,
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
            .changes(
                Change::builder()
                    .action(action)
                    .resource_record_set(
                        ResourceRecordSet::builder()
                            .name(host)
                            .r#type(RrType::Cname)
                            .resource_records(
                                ResourceRecord::builder().value(self.dest.clone()).build(),
                            )
                            .ttl(ttl.unwrap_or(self.ttl))
                            .build(),
                    )
                    .build(),
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

    #[tracing::instrument(skip(self), level = "info")]
    async fn list_records(&self) -> Result<Vec<String>, Self::Error> {
        Ok(self
            .client
            .list_resource_record_sets()
            .hosted_zone_id(self.hosted_zone_id.clone())
            .send()
            .await?
            .resource_record_sets
            .unwrap_or_default()
            .into_iter()
            .filter(|r| {
                // Filter out records that don't match the destination & aren't CNAMEs
                let dest = r
                    .resource_records()
                    .unwrap_or_default()
                    .iter()
                    .find(|v| v.value() == Some(&self.dest));
                r.r#type() == Some(&RrType::Cname) && dest.is_some()
            })
            .filter_map(|r| r.name)
            .map(|mut s| {
                // Remove last dot
                if s.ends_with('.') {
                    s.pop();
                }
                s
            })
            .collect())
    }

    #[tracing::instrument(skip(self), level = "debug")]
    async fn create_record(&self, host: &str) -> Result<(), Self::Error> {
        self.client
            .change_resource_record_sets()
            .hosted_zone_id(self.hosted_zone_id.clone())
            .change_batch(self.change_batch(ChangeAction::Upsert, host, None))
            .send()
            .await?;

        Ok(())
    }

    #[tracing::instrument(skip(self), level = "info")]
    async fn delete_record(&self, host: &str) -> Result<(), Self::Error> {
        let record = self
            .client
            .list_resource_record_sets()
            .hosted_zone_id(self.hosted_zone_id.clone())
            .send()
            .await?
            .resource_record_sets
            .unwrap_or_default()
            .into_iter()
            .find(|r| {
                // Remove last dot & find matching record
                let Some(name) = r.name() else { return false };
                if name.ends_with('.') {
                    let mut chars = name.chars();
                    chars.next_back();
                    chars.as_str() == host && r.r#type() == Some(&RrType::Cname)
                } else {
                    false
                }
            })
            .ok_or(Route53Error::MissingRecord)?;

        self.client
            .change_resource_record_sets()
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

#[cfg(test)]
mod tests {
    use crate::{dns::route53::Route53Provider, dns::Provider};
    use aws_smithy_client::test_connection::TestConnection;
    use aws_smithy_http::body::SdkBody;

    /// Generates a mock client from a list of requests/responses.
    ///
    /// # Arguments
    ///
    /// * `events`: Array of (expected request body, expected response body).
    ///
    /// returns: Client
    fn mock_client(events: Vec<(String, String)>) -> aws_sdk_route53::Client {
        let creds = aws_types::Credentials::from_keys("test", "test", Some("test".to_string()));

        let cfg = aws_sdk_route53::Config::builder()
            .credentials_provider(creds)
            .region(aws_types::region::Region::new("us-east-1"))
            .build();

        let events = events
            .into_iter()
            .map(|(req, res)| {
                let req = http::Request::builder().body(SdkBody::from(req)).unwrap();
                let res = http::Response::builder()
                    .status(200)
                    .body(SdkBody::from(res))
                    .unwrap();
                (req, res)
            })
            .collect();

        let conn = TestConnection::new(events);
        let conn = aws_smithy_client::erase::DynConnector::new(conn);
        aws_sdk_route53::Client::from_conf_conn(cfg, conn)
    }

    #[test]
    fn test_ttl() {
        let client = mock_client(vec![]);
        let mut provider = Route53Provider::new(client, "".to_string(), "".to_string());

        assert_eq!(provider.ttl(), &300);

        *provider.ttl_mut() = 600;

        assert_eq!(provider.ttl(), &600);
    }

    #[test]
    fn test_destination() {
        let client = mock_client(vec![]);
        let mut provider = Route53Provider::new(client, "".to_string(), "dest".to_string());

        assert_eq!(provider.destination(), "dest");

        *provider.destination_mut() = "newdest".to_string();

        assert_eq!(provider.destination(), "newdest");
    }

    #[tokio::test]
    async fn test_list_records() {
        let client = mock_client(vec![(
            r#"{"HostedZoneId": "hosted_zone_id", "MaxItems": "100"}"#.to_string(),
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListResourceRecordSetsResponse>
                    <ResourceRecordSets>
                        <ResourceRecordSet>
                            <Name>test1.example.com.</Name>
                            <Type>CNAME</Type>
                            <TTL>300</TTL>
                            <ResourceRecords>
                                <ResourceRecord>
                                    <Value>dest</Value>
                                </ResourceRecord>
                            </ResourceRecords>
                        </ResourceRecordSet>
                        <ResourceRecordSet>
                            <Name>test2.example.com.</Name>
                            <Type>CNAME</Type>
                            <TTL>300</TTL>
                            <ResourceRecords>
                                <ResourceRecord>
                                    <Value>dest</Value>
                                </ResourceRecord>
                            </ResourceRecords>
                        </ResourceRecordSet>
                        <ResourceRecordSet>
                            <Name>wrong-type.example.com.</Name>
                            <Type>A</Type>
                            <TTL>300</TTL>
                            <ResourceRecords>
                                <ResourceRecord>
                                    <Value>dest</Value>
                                </ResourceRecord>
                            </ResourceRecords>
                        </ResourceRecordSet>
                        <ResourceRecordSet>
                            <Name>wrong-dest.example.com.</Name>
                            <Type>CNAME</Type>
                            <TTL>300</TTL>
                            <ResourceRecords>
                                <ResourceRecord>
                                    <Value>wrong.dest.com</Value>
                                </ResourceRecord>
                            </ResourceRecords>
                        </ResourceRecordSet>
                    </ResourceRecordsSets>
                </ListResourceRecordSetsResponse>
                "#
            .to_string(),
        )]);
        let provider =
            Route53Provider::new(client, "hosted_zone_id".to_string(), "dest".to_string());

        let records = provider.list_records().await.unwrap();

        assert_eq!(records, vec!["test1.example.com", "test2.example.com",]);
    }

    #[tokio::test]
    async fn test_create_record() {
        let client = mock_client(vec![(
            r#"{
                    "HostedZoneId": "hosted_zone_id",
                    "ChangeBatch": {
                        "Changes": [{
                                "Action": "CREATE",
                                "ResourceRecordSet": {
                                    "Name": "test.example.com.",
                                    "Type": "CNAME",
                                    "TTL": 300,
                                    "ResourceRecords": [
                                        {"Value": "dest"}
                                    ]
                                }
                        }]
                    }
                }"#
            .to_string(),
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <ChangeResourceRecordSetsResponse>
                    <ChangeInfo>
                        <Id>change_id</Id>
                    </ChangeInfo>
                </ChangeResourceRecordSetsResponse>
                "#
            .to_string(),
        )]);
        let provider =
            Route53Provider::new(client, "hosted_zone_id".to_string(), "dest".to_string());

        provider.create_record("test.example.com").await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_record() {
        let client = mock_client(vec![
            (
                r#"{"HostedZoneId": "hosted_zone_id", "MaxItems": "100"}"#.to_string(),
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListResourceRecordSetsResponse>
                    <ResourceRecordSets>
                        <ResourceRecordSet>
                            <Name>test.example.com.</Name>
                            <Type>CNAME</Type>
                            <TTL>300</TTL>
                            <ResourceRecords>
                                <ResourceRecord>
                                    <Value>dest</Value>
                                </ResourceRecord>
                            </ResourceRecords>
                        </ResourceRecordSet>
                    </ResourceRecordsSets>
                </ListResourceRecordSetsResponse>
                "#
                .to_string(),
            ),
            (
                r#"{
                    "HostedZoneId": "hosted_zone_id",
                    "ChangeBatch": {
                        "Changes": [{
                                "Action": "DELETE",
                                "ResourceRecordSet": {
                                    "Name": "test.example.com.",
                                    "Type": "CNAME",
                                    "TTL": 300,
                                    "ResourceRecords": [
                                        {"Value": "dest"}
                                    ]
                                }
                        }]
                    }
                }"#
                .to_string(),
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ChangeResourceRecordSetsResponse>
                    <ChangeInfo>
                        <Id>change_id</Id>
                    </ChangeInfo>
                </ChangeResourceRecordSetsResponse>
                "#
                .to_string(),
            ),
        ]);
        let provider =
            Route53Provider::new(client, "hosted_zone_id".to_string(), "dest".to_string());

        provider.delete_record("test.example.com").await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_record_missing() {
        let client = mock_client(vec![
            (
                r#"{"HostedZoneId": "hosted_zone_id", "MaxItems": "100"}"#.to_string(),
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListResourceRecordSetsResponse>
                    <ResourceRecordSets>
                        <ResourceRecordSet>
                            <Name>test.example.com.</Name>
                            <Type>CNAME</Type>
                            <TTL>300</TTL>
                            <ResourceRecords>
                                <ResourceRecord>
                                    <Value>dest</Value>
                                </ResourceRecord>
                            </ResourceRecords>
                        </ResourceRecordSet>
                    </ResourceRecordsSets>
                </ListResourceRecordSetsResponse>
                "#
                .to_string(),
            ),
            (
                r#"{
                    "HostedZoneId": "hosted_zone_id",
                    "ChangeBatch": {
                        "Changes": [{
                                "Action": "DELETE",
                                "ResourceRecordSet": {
                                    "Name": "missing.example.com.",
                                    "Type": "CNAME",
                                    "TTL": 300,
                                    "ResourceRecords": [
                                        {"Value": "dest"}
                                    ]
                                }
                        }]
                    }
                }"#
                .to_string(),
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ChangeResourceRecordSetsResponse>
                    <ChangeInfo>
                        <Id>change_id</Id>
                    </ChangeInfo>
                </ChangeResourceRecordSetsResponse>
                "#
                .to_string(),
            ),
        ]);
        let provider =
            Route53Provider::new(client, "hosted_zone_id".to_string(), "dest".to_string());

        let err = provider
            .delete_record("missing.example.com")
            .await
            .unwrap_err();

        assert!(matches!(err, super::Route53Error::MissingRecord));
    }
}
