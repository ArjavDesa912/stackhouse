//! # Teams & Roles Module (Stackhouse-Teams)
//!
//! Organization and team management with role-based access control.
//! Supports team invitations, role hierarchies, and project permissions.

pub mod orgs;

pub use orgs::*;

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    ReadOnly = 0,
    Audit = 1,
    Developer = 2,
    Admin = 3,
    Owner = 4,
}

#[derive(Clone)]
pub struct TeamsService {
    store: Arc<StackhouseStore>,
}

impl TeamsService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("👥 Stackhouse-Teams initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_teams (
                id BIGSERIAL PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                slug TEXT NOT NULL UNIQUE,
                created_by BIGINT REFERENCES stackhouse_users(id),
                metadata JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_team_members (
                id BIGSERIAL PRIMARY KEY,
                team_id BIGINT NOT NULL REFERENCES stackhouse_teams(id) ON DELETE CASCADE,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id) ON DELETE CASCADE,
                role TEXT NOT NULL DEFAULT 'developer',
                invited_by BIGINT REFERENCES stackhouse_users(id),
                joined_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(team_id, user_id)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_team_invites (
                id BIGSERIAL PRIMARY KEY,
                team_id BIGINT NOT NULL REFERENCES stackhouse_teams(id) ON DELETE CASCADE,
                email TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'developer',
                token TEXT NOT NULL UNIQUE,
                invited_by BIGINT REFERENCES stackhouse_users(id),
                expires_at TIMESTAMPTZ NOT NULL,
                accepted BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    pub async fn create_team(&self, name: &str, owner_id: i64) -> StackhouseResult<i64> {
        let slug = name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();

        let team_id = self
            .store
            .insert_returning_id(
                "INSERT INTO stackhouse_teams (name, slug, created_by) VALUES ($1, $2, $3)"
                    .to_string(),
                vec![
                    SqlValue::Text(name.to_string()),
                    SqlValue::Text(slug),
                    SqlValue::Integer(owner_id),
                ],
            )
            .await?;

        // Add creator as owner
        self.store.execute(
            "INSERT INTO stackhouse_team_members (team_id, user_id, role) VALUES ($1, $2, 'owner')".to_string(),
            vec![SqlValue::Integer(team_id), SqlValue::Integer(owner_id)],
        ).await?;

        Ok(team_id)
    }

    pub async fn list_user_teams(&self, user_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self
            .store
            .query(
                r#"SELECT t.id, t.name, t.slug, tm.role 
               FROM stackhouse_teams t 
               JOIN stackhouse_team_members tm ON t.id = tm.team_id 
               WHERE tm.user_id = $1 
               ORDER BY t.name"#
                    .to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, v);
                }
                Value::Object(obj)
            })
            .collect())
    }

    pub async fn get_team_members(&self, team_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self
            .store
            .query(
                r#"SELECT u.id, u.email, tm.role, tm.joined_at 
               FROM stackhouse_team_members tm 
               JOIN stackhouse_users u ON tm.user_id = u.id 
               WHERE tm.team_id = $1 
               ORDER BY tm.role DESC, u.email"#
                    .to_string(),
                vec![SqlValue::Integer(team_id)],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, v);
                }
                Value::Object(obj)
            })
            .collect())
    }

    pub async fn invite_member(
        &self,
        team_id: i64,
        email: &str,
        role: &str,
        invited_by: i64,
    ) -> StackhouseResult<String> {
        let token = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_team_invites (team_id, email, role, token, invited_by, expires_at) VALUES ($1, $2, $3, $4, $5, NOW() + INTERVAL '7 days')".to_string(),
            vec![
                SqlValue::Integer(team_id),
                SqlValue::Text(email.to_string()),
                SqlValue::Text(role.to_string()),
                SqlValue::Text(token.clone()),
                SqlValue::Integer(invited_by),
            ],
        ).await?;
        Ok(token)
    }

    pub async fn update_member_role(
        &self,
        team_id: i64,
        user_id: i64,
        new_role: &str,
    ) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_team_members SET role = $1 WHERE team_id = $2 AND user_id = $3"
                    .to_string(),
                vec![
                    SqlValue::Text(new_role.to_string()),
                    SqlValue::Integer(team_id),
                    SqlValue::Integer(user_id),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn remove_member(&self, team_id: i64, user_id: i64) -> StackhouseResult<()> {
        // Prevent removing the last owner
        let owners = self.store.query(
            "SELECT COUNT(*) as cnt FROM stackhouse_team_members WHERE team_id = $1 AND role = 'owner'".to_string(),
            vec![SqlValue::Integer(team_id)],
        ).await?;

        let owner_count = owners
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        // Check if user is owner
        let member = self
            .store
            .query(
                "SELECT role FROM stackhouse_team_members WHERE team_id = $1 AND user_id = $2"
                    .to_string(),
                vec![SqlValue::Integer(team_id), SqlValue::Integer(user_id)],
            )
            .await?;

        let is_owner = member
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "role"))
            .and_then(|(_, v)| v.as_str())
            .map(|r| r == "owner")
            .unwrap_or(false);

        if is_owner && owner_count <= 1 {
            return Err(StackhouseError::InvalidPayload(
                "Cannot remove the last owner".to_string(),
            ));
        }

        self.store
            .execute(
                "DELETE FROM stackhouse_team_members WHERE team_id = $1 AND user_id = $2"
                    .to_string(),
                vec![SqlValue::Integer(team_id), SqlValue::Integer(user_id)],
            )
            .await?;
        Ok(())
    }

    pub async fn get_team_member_role(
        &self,
        team_id: i64,
        user_id: i64,
    ) -> StackhouseResult<Option<TeamRole>> {
        let rows = self
            .store
            .query(
                "SELECT role FROM stackhouse_team_members WHERE team_id = $1 AND user_id = $2"
                    .to_string(),
                vec![SqlValue::Integer(team_id), SqlValue::Integer(user_id)],
            )
            .await?;

        let role = rows
            .first()
            .and_then(|row| row.iter().find(|(k, _)| k == "role"))
            .and_then(|(_, v)| v.as_str())
            .and_then(parse_team_role);

        Ok(role)
    }

    pub async fn is_team_member(&self, team_id: i64, user_id: i64) -> StackhouseResult<bool> {
        Ok(self.get_team_member_role(team_id, user_id).await?.is_some())
    }

    pub async fn can_manage_team(&self, team_id: i64, user_id: i64) -> StackhouseResult<bool> {
        Ok(matches!(
            self.get_team_member_role(team_id, user_id).await?,
            Some(TeamRole::Admin) | Some(TeamRole::Owner)
        ))
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct TeamsState {
    pub teams: Arc<TeamsService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct CreateTeamRequest {
    name: String,
}

#[derive(Deserialize)]
struct InviteRequest {
    team_id: i64,
    email: String,
    #[serde(default = "default_developer")]
    role: String,
}
fn default_developer() -> String {
    "developer".to_string()
}

async fn create_team_handler(
    State(state): State<TeamsState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateTeamRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let team_id = state.teams.create_team(&req.name, user.id).await?;
    Ok(Json(json!({"success": true, "data": {"id": team_id}})))
}

async fn list_teams_handler(
    State(state): State<TeamsState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let teams = state.teams.list_user_teams(user.id).await?;
    Ok(Json(json!({"success": true, "data": teams})))
}

async fn team_members_handler(
    State(state): State<TeamsState>,
    headers: HeaderMap,
    axum::extract::Path(team_id): axum::extract::Path<i64>,
) -> Result<impl IntoResponse, StackhouseError> {
    authorize_team_member(&state, team_id, &headers).await?;
    let members = state.teams.get_team_members(team_id).await?;
    Ok(Json(json!({"success": true, "data": members})))
}

async fn invite_handler(
    State(state): State<TeamsState>,
    headers: HeaderMap,
    Json(req): Json<InviteRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    if !state.teams.can_manage_team(req.team_id, user.id).await? {
        return Err(StackhouseError::Forbidden(
            "Team admin access required".to_string(),
        ));
    }
    let token = state
        .teams
        .invite_member(req.team_id, &req.email, &req.role, user.id)
        .await?;
    Ok(Json(
        json!({"success": true, "data": {"invite_token": token}}),
    ))
}

async fn authorize_team_member(
    state: &TeamsState,
    team_id: i64,
    headers: &HeaderMap,
) -> Result<(), StackhouseError> {
    let user = extract_auth_user(&state.auth, headers)?;
    if !state.teams.is_team_member(team_id, user.id).await? {
        return Err(StackhouseError::Forbidden(
            "Team membership required".to_string(),
        ));
    }
    Ok(())
}

fn parse_team_role(role: &str) -> Option<TeamRole> {
    match role {
        "readonly" => Some(TeamRole::ReadOnly),
        "audit" => Some(TeamRole::Audit),
        "developer" => Some(TeamRole::Developer),
        "admin" => Some(TeamRole::Admin),
        "owner" => Some(TeamRole::Owner),
        _ => None,
    }
}

pub fn create_teams_router(state: TeamsState) -> Router {
    Router::new()
        .route("/teams", get(list_teams_handler).post(create_team_handler))
        .route("/teams/:team_id/members", get(team_members_handler))
        .route("/teams/invite", post(invite_handler))
        .with_state(state)
}
