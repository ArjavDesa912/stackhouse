//! # Private Networking — VPC Peering, PrivateLink, Private Endpoints
//!
//! Network isolation for enterprise tenants with private connectivity.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateNetwork {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub network_type: NetworkType,
    pub cidr_block: String,
    pub peer_vpc_id: Option<String>,
    pub peer_account_id: Option<String>,
    pub peer_region: Option<String>,
    pub status: NetworkStatus,
    pub dns_zone: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkType {
    VpcPeering,
    PrivateLink,
    IpsecVpn,
    TransitGateway,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkStatus {
    Pending,
    Active,
    Failed,
    Deleting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateEndpoint {
    pub id: String,
    pub tenant_id: i64,
    pub service_name: String,
    pub endpoint_url: String,
    pub allowed_cidrs: Vec<String>,
    pub status: String,
}

#[derive(Clone)]
pub struct PrivateNetworkService {
    store: Arc<StackhouseStore>,
}

impl PrivateNetworkService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🔒 Private network service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_private_networks (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                network_type TEXT NOT NULL,
                cidr_block TEXT NOT NULL,
                peer_vpc_id TEXT,
                peer_account_id TEXT,
                peer_region TEXT,
                status TEXT DEFAULT 'pending',
                dns_zone TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_private_endpoints (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                service_name TEXT NOT NULL,
                endpoint_url TEXT NOT NULL,
                allowed_cidrs JSONB DEFAULT '[]',
                status TEXT DEFAULT 'pending',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_private_networks_tenant ON stackhouse_private_networks(tenant_id);
        "#.to_string()).await?;
        Ok(())
    }

    pub async fn create_network(
        &self,
        tenant_id: i64,
        name: &str,
        network_type: NetworkType,
        cidr_block: &str,
    ) -> StackhouseResult<PrivateNetwork> {
        let id = uuid::Uuid::new_v4().to_string();
        let type_str = serde_json::to_string(&network_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_private_networks (id, tenant_id, name, network_type, cidr_block, status) VALUES (?, ?, ?, ?, ?, 'pending')".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()), SqlValue::Text(type_str),
                SqlValue::Text(cidr_block.to_string()),
            ],
        ).await?;

        Ok(PrivateNetwork {
            id,
            tenant_id,
            name: name.to_string(),
            network_type,
            cidr_block: cidr_block.to_string(),
            peer_vpc_id: None,
            peer_account_id: None,
            peer_region: None,
            status: NetworkStatus::Pending,
            dns_zone: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub async fn approve_network(&self, network_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_private_networks SET status = 'active' WHERE id = ?".to_string(),
                vec![SqlValue::Text(network_id.to_string())],
            )
            .await?;
        info!("✅ Private network {} approved", network_id);
        Ok(())
    }

    pub async fn create_private_endpoint(
        &self,
        tenant_id: i64,
        service_name: &str,
        allowed_cidrs: Vec<String>,
    ) -> StackhouseResult<PrivateEndpoint> {
        let id = uuid::Uuid::new_v4().to_string();
        let endpoint_url = format!(
            "https://private-{}.stackhouse.internal",
            id[..8].to_string()
        );

        self.store.execute(
            "INSERT INTO stackhouse_private_endpoints (id, tenant_id, service_name, endpoint_url, allowed_cidrs, status) VALUES (?, ?, ?, ?, ?::jsonb, 'active')".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(tenant_id),
                SqlValue::Text(service_name.to_string()), SqlValue::Text(endpoint_url.clone()),
                SqlValue::Text(serde_json::to_string(&allowed_cidrs).unwrap_or_default()),
            ],
        ).await?;

        Ok(PrivateEndpoint {
            id,
            tenant_id,
            service_name: service_name.to_string(),
            endpoint_url,
            allowed_cidrs,
            status: "active".into(),
        })
    }

    pub async fn list_networks(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, network_type, cidr_block, status, created_at FROM stackhouse_private_networks WHERE tenant_id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn is_ip_allowed(&self, tenant_id: i64, ip: &str) -> StackhouseResult<bool> {
        let rows = self.store.query(
            "SELECT allowed_cidrs FROM stackhouse_private_endpoints WHERE tenant_id = ? AND status = 'active'".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        for row in rows {
            let cidrs_str = row
                .iter()
                .find(|(k, _)| k == "allowed_cidrs")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("[]");
            let cidrs: Vec<String> = serde_json::from_str(cidrs_str).unwrap_or_default();
            for cidr in cidrs {
                if Self::ip_in_cidr(ip, &cidr) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
        if cidr == "0.0.0.0/0" {
            return true;
        }

        // Parse IP address into u32
        let ip_parts: Vec<u8> = ip.split('.').filter_map(|s| s.parse().ok()).collect();
        if ip_parts.len() != 4 {
            return false;
        }
        let ip_u32: u32 = ((ip_parts[0] as u32) << 24)
            | ((ip_parts[1] as u32) << 16)
            | ((ip_parts[2] as u32) << 8)
            | (ip_parts[3] as u32);

        // Parse CIDR notation
        if let Some((network, prefix_len_str)) = cidr.split_once('/') {
            let net_parts: Vec<u8> = network.split('.').filter_map(|s| s.parse().ok()).collect();
            if net_parts.len() != 4 {
                return false;
            }
            let net_u32: u32 = ((net_parts[0] as u32) << 24)
                | ((net_parts[1] as u32) << 16)
                | ((net_parts[2] as u32) << 8)
                | (net_parts[3] as u32);

            let prefix_len: u32 = prefix_len_str.parse().unwrap_or(32);
            if prefix_len > 32 {
                return false;
            }

            // Create mask from prefix length
            let mask: u32 = if prefix_len == 0 {
                0
            } else {
                u32::MAX << (32 - prefix_len)
            };

            // Check if IP is in the network range
            return (ip_u32 & mask) == (net_u32 & mask);
        }

        // No prefix — exact match
        ip == cidr
    }
}
