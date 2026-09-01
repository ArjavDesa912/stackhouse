//! # Organization / Workspace Model
//!
//! Enterprise-grade org structure with member roles, invitations,
//! teams/groups, and hierarchical management.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub owner_id: String,
    pub plan_id: String,
    pub branding: OrgBranding,
    pub settings: OrgSettings,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgBranding {
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub favicon_url: Option<String>,
}

impl Default for OrgBranding {
    fn default() -> Self {
        Self {
            logo_url: None,
            primary_color: Some("#3B82F6".into()),
            favicon_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrgSettings {
    pub allow_public_signup: bool,
    pub require_2fa: bool,
    pub max_members: u32,
    pub allow_guest_access: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMember {
    pub id: i64,
    pub org_id: i64,
    pub user_id: String,
    pub role: OrgRole,
    pub invited_by: Option<String>,
    pub joined_at: String,
    pub status: MemberStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    Owner,
    Admin,
    Member,
    Viewer,
    Guest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Active,
    Invited,
    Suspended,
    Left,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgInvitation {
    pub id: String,
    pub org_id: i64,
    pub email: String,
    pub role: OrgRole,
    pub invited_by: String,
    pub token: String,
    pub expires_at: String,
    pub accepted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgTeam {
    pub id: String,
    pub org_id: i64,
    pub name: String,
    pub description: String,
    pub member_ids: Vec<String>,
}

#[derive(Clone)]
pub struct OrgService {
    store: Arc<StackhouseStore>,
}

impl OrgService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🏢 Organization service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_organizations (
                id BIGSERIAL PRIMARY KEY,
                tenant_id BIGINT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                description TEXT DEFAULT '',
                owner_id TEXT NOT NULL,
                plan_id TEXT DEFAULT 'free',
                branding JSONB DEFAULT '{}',
                settings JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_org_members (
                id BIGSERIAL PRIMARY KEY,
                org_id BIGINT NOT NULL,
                user_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'member',
                invited_by TEXT,
                joined_at TIMESTAMPTZ DEFAULT NOW(),
                status TEXT DEFAULT 'active',
                UNIQUE(org_id, user_id)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_org_invitations (
                id TEXT PRIMARY KEY,
                org_id BIGINT NOT NULL,
                email TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'member',
                invited_by TEXT NOT NULL,
                token TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                accepted_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_org_teams (
                id TEXT PRIMARY KEY,
                org_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                member_ids JSONB DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_org_members_org ON stackhouse_org_members(org_id);
            CREATE INDEX IF NOT EXISTS idx_org_invitations_email ON stackhouse_org_invitations(email);
        "#.to_string()).await?;
        Ok(())
    }

    pub async fn create(
        &self,
        tenant_id: i64,
        name: &str,
        slug: &str,
        owner_id: &str,
        plan_id: Option<&str>,
    ) -> StackhouseResult<Organization> {
        let plan = plan_id.unwrap_or("free");
        self.store.execute(
            "INSERT INTO stackhouse_organizations (tenant_id, name, slug, owner_id, plan_id, branding, settings) VALUES (?, ?, ?, ?, ?, ?::jsonb, ?::jsonb)".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(slug.to_string()),
                SqlValue::Text(owner_id.to_string()),
                SqlValue::Text(plan.to_string()),
                SqlValue::Text(serde_json::to_string(&OrgBranding::default()).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&OrgSettings::default()).unwrap_or_default()),
            ],
        ).await?;

        // Add owner as member
        self.store.execute(
            "INSERT INTO stackhouse_org_members (org_id, user_id, role, status) VALUES (?, ?, 'owner', 'active')".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(owner_id.to_string())],
        ).await?;

        Ok(Organization {
            id: tenant_id,
            tenant_id,
            name: name.to_string(),
            slug: slug.to_string(),
            description: String::new(),
            owner_id: owner_id.to_string(),
            plan_id: plan.to_string(),
            branding: OrgBranding::default(),
            settings: OrgSettings::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub async fn get(&self, org_id: i64) -> StackhouseResult<Organization> {
        let rows = self
            .store
            .query(
                "SELECT * FROM stackhouse_organizations WHERE id = ?".to_string(),
                vec![SqlValue::Integer(org_id)],
            )
            .await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Organization not found".into()));
        }
        self.row_to_org(&rows[0])
    }

    pub async fn get_by_tenant(&self, tenant_id: i64) -> StackhouseResult<Option<Organization>> {
        let rows = self
            .store
            .query(
                "SELECT * FROM stackhouse_organizations WHERE tenant_id = ?".to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.row_to_org(&rows[0])?))
    }

    pub async fn update(
        &self,
        org_id: i64,
        name: Option<&str>,
        description: Option<&str>,
        branding: Option<&OrgBranding>,
        settings: Option<&OrgSettings>,
    ) -> StackhouseResult<()> {
        if let Some(n) = name {
            self.store
                .execute(
                    "UPDATE stackhouse_organizations SET name = ? WHERE id = ?".to_string(),
                    vec![SqlValue::Text(n.to_string()), SqlValue::Integer(org_id)],
                )
                .await?;
        }
        if let Some(d) = description {
            self.store
                .execute(
                    "UPDATE stackhouse_organizations SET description = ? WHERE id = ?".to_string(),
                    vec![SqlValue::Text(d.to_string()), SqlValue::Integer(org_id)],
                )
                .await?;
        }
        if let Some(b) = branding {
            self.store
                .execute(
                    "UPDATE stackhouse_organizations SET branding = ?::jsonb WHERE id = ?"
                        .to_string(),
                    vec![
                        SqlValue::Text(serde_json::to_string(b).unwrap_or_default()),
                        SqlValue::Integer(org_id),
                    ],
                )
                .await?;
        }
        if let Some(s) = settings {
            self.store
                .execute(
                    "UPDATE stackhouse_organizations SET settings = ?::jsonb WHERE id = ?"
                        .to_string(),
                    vec![
                        SqlValue::Text(serde_json::to_string(s).unwrap_or_default()),
                        SqlValue::Integer(org_id),
                    ],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn add_member(
        &self,
        org_id: i64,
        user_id: &str,
        role: OrgRole,
        invited_by: Option<&str>,
    ) -> StackhouseResult<OrgMember> {
        let role_str = serde_json::to_string(&role)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_org_members (org_id, user_id, role, invited_by) VALUES (?, ?, ?, ?) ON CONFLICT (org_id, user_id) DO UPDATE SET role = EXCLUDED.role, status = 'active'".to_string(),
            vec![SqlValue::Integer(org_id), SqlValue::Text(user_id.to_string()), SqlValue::Text(role_str), SqlValue::Text(invited_by.unwrap_or("").to_string())],
        ).await?;

        Ok(OrgMember {
            id: 0,
            org_id,
            user_id: user_id.to_string(),
            role,
            invited_by: invited_by.map(|s| s.to_string()),
            joined_at: chrono::Utc::now().to_rfc3339(),
            status: MemberStatus::Active,
        })
    }

    pub async fn remove_member(&self, org_id: i64, user_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_org_members WHERE org_id = ? AND user_id = ?".to_string(),
                vec![
                    SqlValue::Integer(org_id),
                    SqlValue::Text(user_id.to_string()),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn update_member_role(
        &self,
        org_id: i64,
        user_id: &str,
        role: OrgRole,
    ) -> StackhouseResult<()> {
        let role_str = serde_json::to_string(&role)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store
            .execute(
                "UPDATE stackhouse_org_members SET role = ? WHERE org_id = ? AND user_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(role_str),
                    SqlValue::Integer(org_id),
                    SqlValue::Text(user_id.to_string()),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_members(&self, org_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT m.id, m.user_id, m.role, m.joined_at, m.status, u.email FROM stackhouse_org_members m JOIN stackhouse_users u ON m.user_id::bigint = u.id WHERE m.org_id = ?".to_string(),
            vec![SqlValue::Integer(org_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn invite(
        &self,
        org_id: i64,
        email: &str,
        role: OrgRole,
        invited_by: &str,
    ) -> StackhouseResult<OrgInvitation> {
        let id = uuid::Uuid::new_v4().to_string();
        let token = uuid::Uuid::new_v4().to_string();
        let expires = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();
        let role_str = serde_json::to_string(&role)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_org_invitations (id, org_id, email, role, invited_by, token, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(org_id), SqlValue::Text(email.to_string()),
                SqlValue::Text(role_str), SqlValue::Text(invited_by.to_string()),
                SqlValue::Text(token.clone()), SqlValue::Text(expires.clone()),
            ],
        ).await?;

        Ok(OrgInvitation {
            id,
            org_id,
            email: email.to_string(),
            role,
            invited_by: invited_by.to_string(),
            token,
            expires_at: expires,
            accepted_at: None,
        })
    }

    pub async fn accept_invite(&self, token: &str, user_id: &str) -> StackhouseResult<i64> {
        let rows = self.store.query(
            "SELECT org_id, role FROM stackhouse_org_invitations WHERE token = ? AND expires_at > NOW() AND accepted_at IS NULL".to_string(),
            vec![SqlValue::Text(token.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "Invalid or expired invitation".into(),
            ));
        }

        let org_id = rows[0]
            .iter()
            .find(|(k, _)| k == "org_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let role_str = rows[0]
            .iter()
            .find(|(k, _)| k == "role")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("member");
        let role = match role_str {
            "owner" => OrgRole::Owner,
            "admin" => OrgRole::Admin,
            "viewer" => OrgRole::Viewer,
            "guest" => OrgRole::Guest,
            _ => OrgRole::Member,
        };

        self.store
            .execute(
                "UPDATE stackhouse_org_invitations SET accepted_at = NOW() WHERE token = ?"
                    .to_string(),
                vec![SqlValue::Text(token.to_string())],
            )
            .await?;

        self.add_member(org_id, user_id, role, None).await?;
        Ok(org_id)
    }

    pub async fn create_team(
        &self,
        org_id: i64,
        name: &str,
        description: &str,
        member_ids: Vec<String>,
    ) -> StackhouseResult<OrgTeam> {
        let id = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_org_teams (id, org_id, name, description, member_ids) VALUES (?, ?, ?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(org_id),
                SqlValue::Text(name.to_string()), SqlValue::Text(description.to_string()),
                SqlValue::Text(serde_json::to_string(&member_ids).unwrap_or_default()),
            ],
        ).await?;
        Ok(OrgTeam {
            id,
            org_id,
            name: name.to_string(),
            description: description.to_string(),
            member_ids,
        })
    }

    pub async fn list_teams(&self, org_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query("SELECT id, name, description, member_ids FROM stackhouse_org_teams WHERE org_id = ?".to_string(), vec![SqlValue::Integer(org_id)]).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    fn row_to_org(&self, row: &[(String, Value)]) -> StackhouseResult<Organization> {
        let get_str = |key: &str| {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string())
        };
        let get_i64 = |key: &str| {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_i64())
        };

        let branding_str = get_str("branding").unwrap_or_default();
        let settings_str = get_str("settings").unwrap_or_default();

        Ok(Organization {
            id: get_i64("id").unwrap_or(0),
            tenant_id: get_i64("tenant_id").unwrap_or(0),
            name: get_str("name").unwrap_or_default(),
            slug: get_str("slug").unwrap_or_default(),
            description: get_str("description").unwrap_or_default(),
            owner_id: get_str("owner_id").unwrap_or_default(),
            plan_id: get_str("plan_id").unwrap_or_else(|| "free".into()),
            branding: serde_json::from_str(&branding_str).unwrap_or_default(),
            settings: serde_json::from_str(&settings_str).unwrap_or_default(),
            created_at: get_str("created_at").unwrap_or_default(),
        })
    }
}
