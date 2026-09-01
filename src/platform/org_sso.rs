//! # Org-Level SSO Configuration
//!
//! Each organization can bring their own SAML/OIDC Identity Provider.
//! Admins configure SSO per-org, with enforcement and domain verification.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgSsoConfig {
    pub id: String,
    pub org_id: i64,
    pub provider_type: SsoProviderType,
    pub display_name: String,
    pub config: SsoProviderConfig,
    pub domains: Vec<String>,
    pub enforce_sso: bool,
    pub auto_provision: bool,
    pub default_role: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SsoProviderType {
    Saml,
    Oidc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoProviderConfig {
    // SAML fields
    pub idp_entity_id: Option<String>,
    pub idp_sso_url: Option<String>,
    pub idp_certificate: Option<String>,
    pub sp_entity_id: Option<String>,
    pub sp_acs_url: Option<String>,
    // OIDC fields
    pub issuer_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub scopes: Option<Vec<String>>,
    // Attribute mapping
    pub email_attribute: String,
    pub name_attribute: Option<String>,
    pub groups_attribute: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainVerification {
    pub domain: String,
    pub org_id: i64,
    pub verified: bool,
    pub verification_method: VerificationMethod,
    pub verification_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    DnsText,
    DnsCname,
    HttpFile,
}

#[derive(Clone)]
pub struct OrgSsoService {
    store: Arc<StackhouseStore>,
}

impl OrgSsoService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🔐 Org SSO service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_org_sso_configs (
                id TEXT PRIMARY KEY,
                org_id BIGINT NOT NULL,
                provider_type TEXT NOT NULL,
                display_name TEXT NOT NULL,
                config JSONB NOT NULL,
                domains JSONB DEFAULT '[]',
                enforce_sso BOOLEAN DEFAULT FALSE,
                auto_provision BOOLEAN DEFAULT TRUE,
                default_role TEXT DEFAULT 'member',
                enabled BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_org_domain_verifications (
                domain TEXT PRIMARY KEY,
                org_id BIGINT NOT NULL,
                verified BOOLEAN DEFAULT FALSE,
                verification_method TEXT NOT NULL,
                verification_token TEXT NOT NULL,
                verified_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_org_sso_org ON stackhouse_org_sso_configs(org_id);
            CREATE INDEX IF NOT EXISTS idx_org_domains_org ON stackhouse_org_domain_verifications(org_id);
        "#.to_string()).await?;
        Ok(())
    }

    /// Configure SSO for an organization
    pub async fn create_config(&self, config: &OrgSsoConfig) -> StackhouseResult<()> {
        let type_str = serde_json::to_string(&config.provider_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_org_sso_configs (id, org_id, provider_type, display_name, config, domains, enforce_sso, auto_provision, default_role, enabled) VALUES (?, ?, ?, ?, ?::jsonb, ?::jsonb, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(config.id.clone()),
                SqlValue::Integer(config.org_id),
                SqlValue::Text(type_str),
                SqlValue::Text(config.display_name.clone()),
                SqlValue::Text(serde_json::to_string(&config.config).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&config.domains).unwrap_or_default()),
                SqlValue::Text(config.enforce_sso.to_string()),
                SqlValue::Text(config.auto_provision.to_string()),
                SqlValue::Text(config.default_role.clone()),
                SqlValue::Text(config.enabled.to_string()),
            ],
        ).await?;
        Ok(())
    }

    /// Get SSO config for an org
    pub async fn get_config(&self, org_id: i64) -> StackhouseResult<Option<Value>> {
        let rows = self
            .store
            .query(
                "SELECT * FROM stackhouse_org_sso_configs WHERE org_id = ? AND enabled = true"
                    .to_string(),
                vec![SqlValue::Integer(org_id)],
            )
            .await?;
        Ok(rows
            .first()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>())))
    }

    /// Find SSO config by email domain
    pub async fn find_by_domain(&self, email_domain: &str) -> StackhouseResult<Option<Value>> {
        let rows = self
            .store
            .query(
                "SELECT * FROM stackhouse_org_sso_configs WHERE domains ? ? AND enabled = true"
                    .to_string(),
                vec![SqlValue::Text(email_domain.to_string())],
            )
            .await?;
        Ok(rows
            .first()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>())))
    }

    /// Start domain verification
    pub async fn start_domain_verification(
        &self,
        org_id: i64,
        domain: &str,
        method: VerificationMethod,
    ) -> StackhouseResult<DomainVerification> {
        let token = uuid::Uuid::new_v4().to_string().replace("-", "");
        let method_str = serde_json::to_string(&method)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_org_domain_verifications (domain, org_id, verification_method, verification_token) VALUES (?, ?, ?, ?) ON CONFLICT (domain) DO UPDATE SET verification_token = EXCLUDED.verification_token".to_string(),
            vec![
                SqlValue::Text(domain.to_string()),
                SqlValue::Integer(org_id),
                SqlValue::Text(method_str),
                SqlValue::Text(token.clone()),
            ],
        ).await?;

        Ok(DomainVerification {
            domain: domain.to_string(),
            org_id,
            verified: false,
            verification_method: method,
            verification_token: token,
        })
    }

    /// Verify domain ownership via DNS TXT record or HTTP file check
    pub async fn verify_domain(&self, domain: &str) -> StackhouseResult<bool> {
        let rows = self.store.query(
            "SELECT verification_method, verification_token FROM stackhouse_org_domain_verifications WHERE domain = ?".to_string(),
            vec![SqlValue::Text(domain.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "Domain verification not started".into(),
            ));
        }

        let row = &rows[0];
        let method = row
            .iter()
            .find(|(k, _)| k == "verification_method")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("dns_text");
        let token = row
            .iter()
            .find(|(k, _)| k == "verification_token")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");

        if token.is_empty() {
            return Err(StackhouseError::InvalidPayload(
                "No verification token found".into(),
            ));
        }

        let verified = match method {
            "dns_text" => {
                // Query DNS TXT records for _stackhouse-verify.<domain> via DNS-over-HTTPS
                let txt_host = format!("_stackhouse-verify.{}", domain);
                Self::check_dns_txt(&txt_host, token).await
            }
            "dns_cname" => {
                // Check CNAME record points to stackhouse-verify.com
                let cname_host = format!("_stackhouse-verify.{}", domain);
                Self::check_dns_cname(&cname_host, token).await
            }
            "http_file" => {
                // Check HTTP file at https://<domain>/.well-known/stackhouse-verify.txt
                let url = format!("https://{}/.well-known/stackhouse-verify.txt", domain);
                Self::check_http_file(&url, token).await
            }
            _ => false,
        };

        if verified {
            self.store.execute(
                "UPDATE stackhouse_org_domain_verifications SET verified = true, verified_at = NOW() WHERE domain = ?".to_string(),
                vec![SqlValue::Text(domain.to_string())],
            ).await?;
        }

        Ok(verified)
    }

    async fn check_dns_txt(host: &str, expected_token: &str) -> bool {
        // Use Google's DNS-over-HTTPS resolver for TXT record lookup
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let url = format!(
            "https://dns.google/resolve?name={}&type=TXT",
            urlencoding::encode(host)
        );

        let resp = client.get(&url).send().await;
        if let Ok(resp) = resp {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(answers) = body["Answer"].as_array() {
                    for answer in answers {
                        if let Some(data) = answer["data"].as_str() {
                            // TXT records are wrapped in quotes
                            let cleaned = data.trim_matches('"');
                            if cleaned == expected_token {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    async fn check_dns_cname(host: &str, expected_token: &str) -> bool {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let url = format!(
            "https://dns.google/resolve?name={}&type=CNAME",
            urlencoding::encode(host)
        );

        let resp = client.get(&url).send().await;
        if let Ok(resp) = resp {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(answers) = body["Answer"].as_array() {
                    for answer in answers {
                        if let Some(data) = answer["data"].as_str() {
                            let cleaned = data.trim_matches('.');
                            // CNAME should point to stackhouse-verify.com or contain the token
                            if cleaned.contains(expected_token)
                                || cleaned == "stackhouse-verify.com"
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    async fn check_http_file(url: &str, expected_token: &str) -> bool {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let resp = client.get(url).send().await;
        if let Ok(resp) = resp {
            if resp.status().is_success() {
                if let Ok(body) = resp.text().await {
                    if body.trim() == expected_token {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if SSO is enforced for an email domain
    pub async fn is_sso_enforced(&self, email: &str) -> StackhouseResult<bool> {
        let domain = email.split('@').last().unwrap_or("");
        let rows = self.store.query(
            "SELECT enforce_sso FROM stackhouse_org_sso_configs WHERE domains ? ? AND enabled = true".to_string(),
            vec![SqlValue::Text(domain.to_string())],
        ).await?;
        Ok(rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "enforce_sso"))
            .and_then(|(_, v)| v.as_str())
            .map(|s| s == "true" || s == "t")
            .unwrap_or(false))
    }
}
