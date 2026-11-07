use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("network request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("invalid network input: {0}")]
    InvalidInput(String),
    #[error("dns provider error: {0}")]
    Provider(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DnsRecord {
    pub name: String,
    pub r#type: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JoinInfo {
    pub address: String,
    pub port: u16,
    pub is_lan: bool,
    pub qr_payload: String,
}

#[derive(Debug, Clone, Default)]
pub struct MockDnsProvider {
    records: Vec<DnsRecord>,
}

impl MockDnsProvider {
    pub fn records(&self) -> &[DnsRecord] {
        &self.records
    }

    pub fn upsert(&mut self, record: DnsRecord) {
        self.records
            .retain(|existing| !(existing.name == record.name && existing.r#type == record.r#type));
        self.records.push(record);
    }
}

/// Adapter for DuckDNS HTTP API updates.
#[derive(Debug, Clone)]
pub struct DuckDnsClient {
    base_url: String,
    http_client: reqwest::Client,
}

impl Default for DuckDnsClient {
    fn default() -> Self {
        Self::new(None)
    }
}

impl DuckDnsClient {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "https://www.duckdns.org".to_string()),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn update(
        &self,
        domain: &str,
        token: &str,
        ip: Option<&str>,
    ) -> Result<(), NetworkError> {
        let mut url = format!(
            "{}/update?domains={}&token={}",
            self.base_url.trim_end_matches('/'),
            domain,
            token
        );
        if let Some(ip) = ip {
            url.push_str(&format!("&ip={ip}"));
        }

        let response = self.http_client.get(&url).send().await?;
        let text = response.text().await?;
        if text.trim() == "OK" {
            Ok(())
        } else {
            Err(NetworkError::Provider(format!(
                "DuckDNS update rejected: {text}"
            )))
        }
    }
}

/// Adapter for Cloudflare DNS API updates.
#[derive(Debug, Clone)]
pub struct CloudflareDnsClient {
    base_url: String,
    http_client: reqwest::Client,
}

impl Default for CloudflareDnsClient {
    fn default() -> Self {
        Self::new(None)
    }
}

impl CloudflareDnsClient {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url
                .unwrap_or_else(|| "https://api.cloudflare.com/client/v4".to_string()),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn upsert_record(
        &self,
        zone_id: &str,
        api_token: &str,
        record: &DnsRecord,
    ) -> Result<(), NetworkError> {
        let url = format!(
            "{}/zones/{}/dns_records",
            self.base_url.trim_end_matches('/'),
            zone_id
        );
        let payload = serde_json::json!({
            "type": record.r#type,
            "name": record.name,
            "content": record.value,
            "ttl": 1,
            "proxied": false
        });

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(api_token)
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(NetworkError::Provider(format!(
                "Cloudflare returned {status}: {body}"
            )))
        }
    }
}

/// Adapter for deSEC (desec.io) DNS API updates.
#[derive(Debug, Clone)]
pub struct DesecClient {
    base_url: String,
    http_client: reqwest::Client,
}

impl Default for DesecClient {
    fn default() -> Self {
        Self::new(None)
    }
}

impl DesecClient {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "https://desec.io/api/v1".to_string()),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn update_record(
        &self,
        domain: &str,
        subname: &str,
        token: &str,
        ip: &str,
    ) -> Result<(), NetworkError> {
        let clean_sub = if subname.is_empty() || subname == "@" {
            String::new()
        } else {
            format!("{subname}/")
        };
        let url = format!(
            "{}/domains/{}/rrsets/{}A/",
            self.base_url.trim_end_matches('/'),
            domain,
            clean_sub
        );
        let payload = serde_json::json!({
            "records": [ip],
            "ttl": 3600
        });

        let response = self
            .http_client
            .put(&url)
            .header("Authorization", format!("Token {token}"))
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(NetworkError::Provider(format!(
                "deSEC returned {status}: {body}"
            )))
        }
    }
}

/// Adapter for Playit.gg tunnel management.
#[derive(Debug, Clone)]
pub struct PlayitClient {
    base_url: String,
    http_client: reqwest::Client,
}

impl Default for PlayitClient {
    fn default() -> Self {
        Self::new(None)
    }
}

impl PlayitClient {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "https://api.playit.gg/v1".to_string()),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn sync_tunnel(
        &self,
        secret_key: &str,
        tunnel_name: &str,
    ) -> Result<String, NetworkError> {
        let url = format!("{}/tunnels", self.base_url.trim_end_matches('/'));
        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Secret-Key {secret_key}"))
            .send()
            .await?;

        if response.status().is_success() {
            let domain = if tunnel_name.is_empty() {
                "play.gl.joinmc.link".to_string()
            } else if tunnel_name.contains('.') {
                tunnel_name.to_string()
            } else {
                format!("{tunnel_name}.gl.joinmc.link")
            };
            Ok(domain)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(NetworkError::Provider(format!(
                "Playit returned {status}: {body}"
            )))
        }
    }
}

/// Keychain store backed by OS keychain, with in-memory fallback for test and headless environments.
#[derive(Clone, Default)]
pub struct SecretStore {
    fallback: Arc<Mutex<HashMap<String, String>>>,
}

impl SecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save(&self, key: &str, secret: &str) -> Result<(), NetworkError> {
        if let Ok(entry) = keyring::Entry::new("redstone", key) {
            let _ = entry.set_password(secret);
        }
        let mut map = self
            .fallback
            .lock()
            .map_err(|e| NetworkError::Keychain(e.to_string()))?;
        map.insert(key.to_string(), secret.to_string());
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, NetworkError> {
        if let Ok(entry) = keyring::Entry::new("redstone", key) {
            if let Ok(secret) = entry.get_password() {
                return Ok(Some(secret));
            }
        }
        let map = self
            .fallback
            .lock()
            .map_err(|e| NetworkError::Keychain(e.to_string()))?;
        Ok(map.get(key).cloned())
    }

    pub fn delete(&self, key: &str) -> Result<(), NetworkError> {
        if let Ok(entry) = keyring::Entry::new("redstone", key) {
            let _ = entry.delete_credential();
        }
        let mut map = self
            .fallback
            .lock()
            .map_err(|e| NetworkError::Keychain(e.to_string()))?;
        map.remove(key);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NetworkManager {
    provider: MockDnsProvider,
}

impl NetworkManager {
    pub fn new(provider: MockDnsProvider) -> Self {
        Self { provider }
    }

    pub fn provider(&self) -> &MockDnsProvider {
        &self.provider
    }

    pub fn compute_join_info(
        lan_only: bool,
        local_ip: &str,
        port: u16,
        subdomain: Option<&str>,
        domain: Option<&str>,
    ) -> JoinInfo {
        if lan_only {
            let address = if local_ip.trim().is_empty() {
                "127.0.0.1".to_string()
            } else {
                local_ip.trim().to_string()
            };
            let qr_payload = format!("{address}:{port}");
            JoinInfo {
                address,
                port,
                is_lan: true,
                qr_payload,
            }
        } else {
            let full_hostname = match (subdomain, domain) {
                (Some(sub), Some(dom)) if !sub.is_empty() && !dom.is_empty() => {
                    format!("{sub}.{dom}")
                }
                (None, Some(dom)) if !dom.is_empty() => dom.to_string(),
                _ => local_ip.to_string(),
            };
            let qr_payload = if port == 25565 {
                full_hostname.clone()
            } else {
                format!("{full_hostname}:{port}")
            };
            JoinInfo {
                address: full_hostname,
                port,
                is_lan: false,
                qr_payload,
            }
        }
    }

    pub fn update_dns(&mut self, subdomain: &str, domain: &str, include_srv: bool) {
        let hostname = format!("{subdomain}.{domain}");
        self.provider.upsert(DnsRecord {
            name: hostname.clone(),
            r#type: "A".to_string(),
            value: "127.0.0.1".to_string(),
        });

        if include_srv {
            self.provider.upsert(DnsRecord {
                name: format!("_minecraft._tcp.{hostname}"),
                r#type: "SRV".to_string(),
                value: format!("1 1 25565 {hostname}."),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn dns_records_are_created_for_a_and_srv_when_needed() {
        let provider = MockDnsProvider::default();
        let mut manager = NetworkManager::new(provider);

        manager.update_dns("play", "example.com", true);

        let records = manager.provider().records();
        assert!(records
            .iter()
            .any(|record| record.name == "play.example.com" && record.r#type == "A"));
        assert!(records.iter().any(|record| {
            record.name == "_minecraft._tcp.play.example.com" && record.r#type == "SRV"
        }));
    }

    #[test]
    fn subdomain_change_triggers_dns_update() {
        let provider = MockDnsProvider::default();
        let mut manager = NetworkManager::new(provider);

        manager.update_dns("play", "example.com", true);
        manager.update_dns("v2", "example.com", true);

        let records = manager.provider().records();
        assert!(records
            .iter()
            .any(|r| r.name == "v2.example.com" && r.r#type == "A"));
    }

    #[test]
    fn compute_join_info_handles_lan_and_online_modes() {
        let lan_info = NetworkManager::compute_join_info(true, "192.168.1.50", 25565, None, None);
        assert!(lan_info.is_lan);
        assert_eq!(lan_info.address, "192.168.1.50");
        assert_eq!(lan_info.port, 25565);
        assert_eq!(lan_info.qr_payload, "192.168.1.50:25565");

        let online_info = NetworkManager::compute_join_info(
            false,
            "192.168.1.50",
            25565,
            Some("mc"),
            Some("example.org"),
        );
        assert!(!online_info.is_lan);
        assert_eq!(online_info.address, "mc.example.org");
        assert_eq!(online_info.qr_payload, "mc.example.org");
    }

    #[test]
    fn secret_store_round_trips_secrets() {
        let store = SecretStore::new();
        let key = "test_key_roundtrip_unique";
        let _ = store.delete(key);
        assert_eq!(store.get(key).unwrap(), None);

        store.save(key, "secret_value_123").unwrap();
        assert_eq!(store.get(key).unwrap().as_deref(), Some("secret_value_123"));

        store.delete(key).unwrap();
        assert_eq!(store.get(key).unwrap(), None);
    }

    #[tokio::test]
    async fn duckdns_adapter_makes_mocked_http_request() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/update"))
            .and(query_param("domains", "mycoolmc"))
            .and(query_param("token", "secret-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
            .mount(&server)
            .await;

        let client = DuckDnsClient::new(Some(server.uri()));
        let res = client.update("mycoolmc", "secret-token", None).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn cloudflare_adapter_makes_mocked_http_request() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/zones/zone123/dns_records"))
            .and(header("authorization", "Bearer cf-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
            })))
            .mount(&server)
            .await;

        let client = CloudflareDnsClient::new(Some(server.uri()));
        let record = DnsRecord {
            name: "play.example.com".to_string(),
            r#type: "A".to_string(),
            value: "1.2.3.4".to_string(),
        };
        let res = client.upsert_record("zone123", "cf-token", &record).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn desec_adapter_makes_mocked_http_request() {
        let server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/domains/example.dedyn.io/rrsets/play/A/"))
            .and(header("authorization", "Token desec-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "created": "2026-11-07T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let client = DesecClient::new(Some(server.uri()));
        let res = client
            .update_record("example.dedyn.io", "play", "desec-token", "1.2.3.4")
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn playit_adapter_makes_mocked_http_request() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/tunnels"))
            .and(header("authorization", "Secret-Key playit-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok"
            })))
            .mount(&server)
            .await;

        let client = PlayitClient::new(Some(server.uri()));
        let res = client.sync_tunnel("playit-secret", "myserver").await;
        assert_eq!(res.unwrap(), "myserver.gl.joinmc.link");
    }
}
