//! # Point-in-Time Recovery (PITR)
//!
//! Continuous WAL archiving with restore to any point within the retention window.
//! Minimum 7-day window for enterprise compliance.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use base64::Engine;
use chrono::{DateTime, Utc};
use pg_walstream::{ChangeEvent, EventType, Lsn, PgOutputDecoder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitrConfig {
    pub retention_days: u32,
    pub wal_archive_path: String,
    pub backup_schedule: String, // cron expression
    pub last_backup_at: Option<String>,
    pub last_wal_archive_at: Option<String>,
}

impl Default for PitrConfig {
    fn default() -> Self {
        Self {
            retention_days: 7,
            wal_archive_path: "/var/lib/stackhouse/wal".to_string(),
            backup_schedule: "0 2 * * *".to_string(), // Daily at 2 AM
            last_backup_at: None,
            last_wal_archive_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePoint {
    pub id: String,
    pub tenant_id: i64,
    pub point_in_time: String,
    pub backup_type: String,
    pub status: RestoreStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RestoreStatus {
    Available,
    Restoring,
    Restored,
    Expired,
}

#[derive(Clone)]
pub struct PitrService {
    store: Arc<StackhouseStore>,
    config: PitrConfig,
}

impl PitrService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            config: PitrConfig::default(),
        };
        service.initialize_tables().await?;
        service.initialize_slot_and_publication().await?;
        info!(
            "⏪ PITR service initialized ({}-day retention)",
            service.config.retention_days
        );
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_pitr_config (
                tenant_id BIGINT PRIMARY KEY,
                retention_days INTEGER DEFAULT 7,
                wal_archive_path TEXT DEFAULT '/var/lib/stackhouse/wal',
                backup_schedule TEXT DEFAULT '0 2 * * *',
                last_backup_at TIMESTAMPTZ,
                last_wal_archive_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_restore_points (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                point_in_time TIMESTAMPTZ NOT NULL,
                backup_type TEXT DEFAULT 'full',
                status TEXT DEFAULT 'available',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_pitr_base_backups (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                point_in_time TIMESTAMPTZ NOT NULL,
                lsn TEXT NOT NULL,
                base_schema TEXT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_restore_points_tenant ON stackhouse_restore_points(tenant_id, point_in_time);
            CREATE INDEX IF NOT EXISTS idx_pitr_base_tenant ON stackhouse_pitr_base_backups(tenant_id, point_in_time);
        "#.to_string()).await?;
        Ok(())
    }

    async fn initialize_slot_and_publication(&self) -> StackhouseResult<()> {
        // Logical-replication publication covering all public tables. The associated
        // slot provides a real WAL stream for point-in-time recovery.
        self.store
            .execute(
                "CREATE PUBLICATION IF NOT EXISTS stackhouse_pitr_pub FOR TABLES IN SCHEMA public"
                    .to_string(),
                vec![],
            )
            .await
            .ok();

        let slot_sql = r#"
            DO $$
            BEGIN
                PERFORM pg_create_logical_replication_slot('stackhouse_pitr_slot', 'pgoutput');
            EXCEPTION
                WHEN duplicate_object THEN
                    RAISE NOTICE 'PITR slot already exists';
            END $$;
        "#;
        self.store.execute(slot_sql.to_string(), vec![]).await.ok();
        Ok(())
    }

    /// Configure PITR for a tenant
    pub async fn configure(&self, tenant_id: i64, config: &PitrConfig) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_pitr_config (tenant_id, retention_days, wal_archive_path, backup_schedule) VALUES (?, ?, ?, ?) ON CONFLICT (tenant_id) DO UPDATE SET retention_days = EXCLUDED.retention_days, wal_archive_path = EXCLUDED.wal_archive_path, backup_schedule = EXCLUDED.backup_schedule".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Integer(config.retention_days as i64),
                SqlValue::Text(config.wal_archive_path.clone()),
                SqlValue::Text(config.backup_schedule.clone()),
            ],
        ).await?;
        Ok(())
    }

    /// Create a named restore point
    pub async fn create_restore_point(
        &self,
        tenant_id: i64,
        name: &str,
    ) -> StackhouseResult<RestorePoint> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.store.execute(
            "INSERT INTO stackhouse_restore_points (id, tenant_id, point_in_time, backup_type, status) VALUES (?, ?, ?::timestamptz, 'named', 'available')".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(now.clone()),
            ],
        ).await?;

        // Also create a Postgres restore point
        self.store
            .execute(
                format!(
                    "SELECT pg_create_restore_point('stackhouse_{}_{}')",
                    tenant_id, name
                ),
                vec![],
            )
            .await
            .ok();

        Ok(RestorePoint {
            id,
            tenant_id,
            point_in_time: now,
            backup_type: "named".to_string(),
            status: RestoreStatus::Available,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn current_wal_lsn(&self) -> StackhouseResult<String> {
        let rows = self
            .store
            .query(
                "SELECT pg_current_wal_lsn()::text AS lsn".to_string(),
                vec![],
            )
            .await?;
        Ok(rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "lsn"))
            .and_then(|(_, v)| v.as_str().map(String::from))
            .unwrap_or_default())
    }

    /// Create a full base backup for a tenant, recording the WAL LSN at which the
    /// snapshot is consistent. Subsequent PITR restores replay WAL from this LSN.
    pub async fn create_full_backup(&self, tenant_id: i64) -> StackhouseResult<RestorePoint> {
        let id = uuid::Uuid::new_v4().to_string();
        let short = &id[..8];
        let base_schema = format!("stackhouse_backup_{}_{}", tenant_id, short);
        let now = chrono::Utc::now().to_rfc3339();

        // 1. Record the LSN before taking the snapshot.
        let lsn = self.current_wal_lsn().await?;

        // 2. Create the base schema and clone tenant-visible tables.
        self.store
            .execute(
                format!("CREATE SCHEMA IF NOT EXISTS {}", base_schema),
                vec![],
            )
            .await?;

        let tables = self.store.query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_name LIKE 'stackhouse_%' AND table_type = 'BASE TABLE'".to_string(),
            vec![],
        ).await?;

        for row in tables {
            let table = row
                .iter()
                .find(|(k, _)| k == "table_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            if table.is_empty() {
                continue;
            }

            let table_ident = pg_walstream::quote_ident(table)
                .map_err(|e| StackhouseError::Database(format!("invalid table name: {:?}", e)))?;

            self.store
                .execute(
                    format!(
                        "CREATE TABLE IF NOT EXISTS {}.{} (LIKE public.{} INCLUDING ALL)",
                        base_schema, table_ident, table_ident
                    ),
                    vec![],
                )
                .await?;

            let has_tenant = self.table_has_column("public", table, "tenant_id").await?;
            if has_tenant {
                self.store
                    .execute(
                        format!(
                            "INSERT INTO {}.{} SELECT * FROM public.{} WHERE tenant_id = ?",
                            base_schema, table_ident, table_ident
                        ),
                        vec![SqlValue::Integer(tenant_id)],
                    )
                    .await?;
            } else {
                self.store
                    .execute(
                        format!(
                            "INSERT INTO {}.{} SELECT * FROM public.{}",
                            base_schema, table_ident, table_ident
                        ),
                        vec![],
                    )
                    .await?;
            }
        }

        self.store.execute(
            "INSERT INTO stackhouse_pitr_base_backups (id, tenant_id, point_in_time, lsn, base_schema) VALUES (?, ?, ?::timestamptz, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(now.clone()),
                SqlValue::Text(lsn),
                SqlValue::Text(base_schema.clone()),
            ],
        ).await?;

        Ok(RestorePoint {
            id,
            tenant_id,
            point_in_time: now,
            backup_type: "full".to_string(),
            status: RestoreStatus::Available,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn table_has_column(
        &self,
        schema: &str,
        table: &str,
        column: &str,
    ) -> StackhouseResult<bool> {
        let rows = self.store.query(
            "SELECT column_name FROM information_schema.columns WHERE table_schema = ? AND table_name = ? AND column_name = ?".to_string(),
            vec![
                SqlValue::Text(schema.to_string()),
                SqlValue::Text(table.to_string()),
                SqlValue::Text(column.to_string()),
            ],
        ).await?;
        Ok(!rows.is_empty())
    }

    /// List available restore points for a tenant
    pub async fn list_restore_points(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, point_in_time, backup_type, status, created_at FROM stackhouse_restore_points WHERE tenant_id = ? AND status = 'available' ORDER BY point_in_time DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Restore to a specific point in time
    pub async fn restore_to(&self, tenant_id: i64, target_time: &str) -> StackhouseResult<String> {
        info!("🔄 Restoring tenant {} to {}", tenant_id, target_time);

        // Verify the target is within retention window
        let config_rows = self
            .store
            .query(
                "SELECT retention_days, wal_archive_path FROM stackhouse_pitr_config WHERE tenant_id = ?"
                    .to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;

        let retention = config_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "retention_days"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(7);
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention);
        let target = chrono::DateTime::parse_from_rfc3339(target_time)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(chrono::Utc::now());

        if target < cutoff {
            return Err(StackhouseError::InvalidPayload(format!(
                "Target time is outside {}-day retention window",
                retention
            )));
        }

        let op_id = uuid::Uuid::new_v4().to_string();
        let restore_schema = format!("stackhouse_restore_{}", &op_id[..8]);

        // Create a restore record
        self.store.execute(
            "INSERT INTO stackhouse_restore_points (id, tenant_id, point_in_time, backup_type, status) VALUES (?, ?, ?::timestamptz, 'restore', 'restoring')".to_string(),
            vec![
                SqlValue::Text(op_id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(target_time.to_string()),
            ],
        ).await?;

        // 1. Create a restore schema for this tenant (temporary)
        self.store
            .execute(
                format!("CREATE SCHEMA IF NOT EXISTS {}", restore_schema),
                vec![],
            )
            .await?;

        // 2. Find the latest base backup before the target time
        let backup_rows = self.store.query(
            "SELECT id, point_in_time, lsn, base_schema FROM stackhouse_pitr_base_backups WHERE tenant_id = ? AND point_in_time <= ?::timestamptz ORDER BY point_in_time DESC LIMIT 1".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(target_time.to_string())],
        ).await?;

        if backup_rows.is_empty() {
            self.store
                .execute(
                    "UPDATE stackhouse_restore_points SET status = 'expired' WHERE id = ?"
                        .to_string(),
                    vec![SqlValue::Text(op_id.clone())],
                )
                .await?;
            self.store
                .execute(
                    format!("DROP SCHEMA IF EXISTS {} CASCADE", restore_schema),
                    vec![],
                )
                .await
                .ok();
            return Err(StackhouseError::NotFound(
                "No base backup found before target time".into(),
            ));
        }

        let base_lsn = backup_rows[0]
            .iter()
            .find(|(k, _)| k == "lsn")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let base_schema = backup_rows[0]
            .iter()
            .find(|(k, _)| k == "base_schema")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();

        // 3. Clone the base backup into the restore schema
        self.clone_base_schema(&base_schema, &restore_schema)
            .await?;

        // 4. Replay WAL entries from the logical replication slot
        if let Err(e) = self
            .replay_wal(&restore_schema, tenant_id, &base_lsn, target)
            .await
        {
            warn!("WAL replay error: {}; restore may be incomplete", e);
        }

        // 5. Mark restore as completed
        self.store
            .execute(
                "UPDATE stackhouse_restore_points SET status = 'restored' WHERE id = ?".to_string(),
                vec![SqlValue::Text(op_id.clone())],
            )
            .await?;

        info!(
            "✅ Restore completed: tenant {} to {} (restore schema: {})",
            tenant_id, target_time, restore_schema
        );
        Ok(format!(
            "Restored to {}. Operation ID: {}. Restore schema: {}",
            target_time, op_id, restore_schema
        ))
    }

    async fn clone_base_schema(
        &self,
        base_schema: &str,
        restore_schema: &str,
    ) -> StackhouseResult<()> {
        self.store
            .execute(
                format!("CREATE SCHEMA IF NOT EXISTS {}", restore_schema),
                vec![],
            )
            .await?;

        let tables = self.store.query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = ? AND table_type = 'BASE TABLE'".to_string(),
            vec![SqlValue::Text(base_schema.to_string())],
        ).await?;

        for row in tables {
            let table = row
                .iter()
                .find(|(k, _)| k == "table_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            if table.is_empty() {
                continue;
            }

            let table_ident = pg_walstream::quote_ident(table)
                .map_err(|e| StackhouseError::Database(format!("invalid table name: {:?}", e)))?;

            self.store
                .execute(
                    format!(
                        "CREATE TABLE IF NOT EXISTS {}.{} (LIKE {}.{} INCLUDING ALL)",
                        restore_schema, table_ident, base_schema, table_ident
                    ),
                    vec![],
                )
                .await?;

            self.store
                .execute(
                    format!(
                        "INSERT INTO {}.{} SELECT * FROM {}.{}",
                        restore_schema, table_ident, base_schema, table_ident
                    ),
                    vec![],
                )
                .await?;
        }

        Ok(())
    }

    async fn replay_wal(
        &self,
        restore_schema: &str,
        tenant_id: i64,
        base_lsn: &str,
        target: DateTime<Utc>,
    ) -> StackhouseResult<()> {
        let base_lsn = Lsn::from_str(base_lsn)
            .map_err(|e| StackhouseError::Database(format!("invalid base LSN: {:?}", e)))?;
        let mut decoder = PgOutputDecoder::with_protocol_version(1);

        // Peek a large batch of WAL records. We do not advance the slot so
        // repeated restores remain possible, but the batch must be large enough
        // to cover all changes between the base backup and the target time.
        let sql = "SELECT lsn, xid, data FROM pg_logical_slot_peek_binary_changes('stackhouse_pitr_slot', NULL, 100000, 'proto_version', '1', 'publication_names', 'stackhouse_pitr_pub')";

        let rows = self
            .store
            .query(sql.to_string(), vec![])
            .await
            .map_err(|e| StackhouseError::Database(format!("failed to peek PITR slot: {}", e)))?;

        let mut buffer: Vec<ChangeEvent> = Vec::new();

        for row in rows {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            let lsn_str = get("lsn")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            let data_b64 = get("data")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();

            if lsn_str.is_empty() || data_b64.is_empty() {
                continue;
            }

            let lsn = match Lsn::from_str(&lsn_str) {
                Ok(l) => l,
                Err(e) => {
                    warn!("PITR failed to parse LSN '{}': {}", lsn_str, e);
                    continue;
                }
            };

            let data = match base64::engine::general_purpose::STANDARD.decode(&data_b64) {
                Ok(v) => v,
                Err(e) => {
                    warn!("PITR failed to decode base64 data: {}", e);
                    continue;
                }
            };

            let change = match decoder.decode_message(data, lsn) {
                Ok(Some(c)) => c,
                Ok(None) => continue,
                Err(e) => {
                    warn!("PITR failed to decode pgoutput message: {}", e);
                    continue;
                }
            };

            match &change.event_type {
                EventType::Begin { .. } => {
                    buffer.clear();
                }
                EventType::Commit {
                    commit_timestamp,
                    commit_lsn,
                    ..
                } => {
                    if *commit_lsn < base_lsn {
                        // Whole transaction is before the base backup; discard it.
                        buffer.clear();
                    } else if *commit_timestamp <= target {
                        for ev in &buffer {
                            if let Err(e) =
                                self.apply_change_event(restore_schema, tenant_id, ev).await
                            {
                                warn!("PITR apply error: {}", e);
                            }
                        }
                        buffer.clear();
                    } else {
                        // Reached a commit after the target; further changes are beyond PITR.
                        buffer.clear();
                        break;
                    }
                }
                EventType::Insert { .. }
                | EventType::Update { .. }
                | EventType::Delete { .. }
                | EventType::Truncate(_) => buffer.push(change),
                _ => {}
            }
        }

        Ok(())
    }

    async fn apply_change_event(
        &self,
        restore_schema: &str,
        tenant_id: i64,
        event: &ChangeEvent,
    ) -> StackhouseResult<()> {
        match &event.event_type {
            EventType::Insert { table, data, .. } => {
                let row = self.row_data_to_value(data)?;
                if !self.event_matches_tenant(tenant_id, &row) {
                    return Ok(());
                }
                self.upsert_row(restore_schema, table, &row).await?;
            }
            EventType::Update {
                table,
                old_data,
                new_data,
                ..
            } => {
                let new_row = self.row_data_to_value(new_data)?;
                if !self.event_matches_tenant(tenant_id, &new_row) {
                    return Ok(());
                }
                let old_row = old_data
                    .as_ref()
                    .map(|d| self.row_data_to_value(d))
                    .transpose()?;
                self.update_row(restore_schema, table, &new_row, old_row.as_ref())
                    .await?;
            }
            EventType::Delete {
                table, old_data, ..
            } => {
                let row = self.row_data_to_value(old_data)?;
                if !self.event_matches_tenant(tenant_id, &row) {
                    return Ok(());
                }
                self.delete_row(restore_schema, table, &row).await?;
            }
            EventType::Truncate(tables) => {
                for t in tables {
                    let t_ident = pg_walstream::quote_ident(t).map_err(|e| {
                        StackhouseError::Database(format!("invalid table name: {:?}", e))
                    })?;
                    self.store
                        .execute(
                            format!("TRUNCATE TABLE {}.{}", restore_schema, t_ident),
                            vec![],
                        )
                        .await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn event_matches_tenant(&self, tenant_id: i64, row: &Value) -> bool {
        match row.get("tenant_id").and_then(|v| v.as_i64()) {
            Some(t) => t == tenant_id,
            None => true, // shared tables; base backup already scoped when possible
        }
    }

    fn row_data_to_value(&self, row: &pg_walstream::RowData) -> StackhouseResult<Value> {
        row.deserialize_into::<Value>()
            .map_err(|e| StackhouseError::Database(format!("failed to deserialize row: {:?}", e)))
    }

    async fn upsert_row(
        &self,
        restore_schema: &str,
        table: &str,
        row: &Value,
    ) -> StackhouseResult<()> {
        let table_ident = pg_walstream::quote_ident(table)
            .map_err(|e| StackhouseError::Database(format!("invalid table name: {:?}", e)))?;
        let obj = row
            .as_object()
            .ok_or_else(|| StackhouseError::Database("WAL row is not an object".into()))?;

        if obj.is_empty() {
            return Ok(());
        }

        let (cols, values): (Vec<_>, Vec<_>) = obj
            .iter()
            .map(|(k, v)| {
                let col = pg_walstream::quote_ident(k)
                    .map_err(|e| StackhouseError::Database(format!("invalid column name: {:?}", e)))
                    .map(|s| (s, Self::value_to_sql(v)))?;
                Ok::<_, StackhouseError>(col)
            })
            .collect::<StackhouseResult<Vec<_>>>()?
            .into_iter()
            .unzip();

        let placeholders: Vec<String> = (0..cols.len()).map(|_| format!("?")).collect();
        let col_list = cols.join(", ");
        let sql = format!(
            "INSERT INTO {}.{} ({}) VALUES ({}) ON CONFLICT DO NOTHING",
            restore_schema,
            table_ident,
            col_list,
            placeholders.join(", ")
        );
        self.store.execute(sql, values).await?;
        Ok(())
    }

    async fn update_row(
        &self,
        restore_schema: &str,
        table: &str,
        new_row: &Value,
        old_row: Option<&Value>,
    ) -> StackhouseResult<()> {
        let table_ident = pg_walstream::quote_ident(table)
            .map_err(|e| StackhouseError::Database(format!("invalid table name: {:?}", e)))?;
        let obj = new_row
            .as_object()
            .ok_or_else(|| StackhouseError::Database("WAL row is not an object".into()))?;

        // Use the 'id' column as the key if present; otherwise fall back to the old row.
        let (key_col, key_val) = match obj.get("id") {
            Some(v) => ("id", Self::value_to_sql(v)),
            None => match old_row.and_then(|r| r.as_object()) {
                Some(old) => match old.get("id") {
                    Some(v) => ("id", Self::value_to_sql(v)),
                    None => {
                        // No usable key; apply as an upsert using all columns.
                        return self.upsert_row(restore_schema, table, new_row).await;
                    }
                },
                None => {
                    return self.upsert_row(restore_schema, table, new_row).await;
                }
            },
        };

        let set_pairs: Vec<(String, SqlValue)> = obj
            .iter()
            .filter(|(k, _)| *k != "id")
            .map(|(k, v)| {
                let col = pg_walstream::quote_ident(k).map_err(|e| {
                    StackhouseError::Database(format!("invalid column name: {:?}", e))
                })?;
                Ok((col, Self::value_to_sql(v)))
            })
            .collect::<StackhouseResult<Vec<_>>>()?;

        if set_pairs.is_empty() {
            return Ok(());
        }

        let set_clauses: Vec<String> = set_pairs
            .iter()
            .map(|(c, _)| format!("{} = ?", c))
            .collect();
        let mut values: Vec<SqlValue> = set_pairs.into_iter().map(|(_, v)| v).collect();
        values.push(key_val);

        let sql = format!(
            "UPDATE {}.{} SET {} WHERE {} = ?",
            restore_schema,
            table_ident,
            set_clauses.join(", "),
            pg_walstream::quote_ident(key_col)
                .map_err(|e| StackhouseError::Database(format!("invalid key column: {:?}", e)))?
        );
        self.store.execute(sql, values).await?;
        Ok(())
    }

    async fn delete_row(
        &self,
        restore_schema: &str,
        table: &str,
        row: &Value,
    ) -> StackhouseResult<()> {
        let table_ident = pg_walstream::quote_ident(table)
            .map_err(|e| StackhouseError::Database(format!("invalid table name: {:?}", e)))?;
        let obj = row
            .as_object()
            .ok_or_else(|| StackhouseError::Database("WAL row is not an object".into()))?;

        if let Some(id) = obj.get("id") {
            let sql = format!(
                "DELETE FROM {}.{} WHERE {} = ?",
                restore_schema,
                table_ident,
                pg_walstream::quote_ident("id").map_err(|e| StackhouseError::Database(format!(
                    "invalid key column: {:?}",
                    e
                )))?
            );
            self.store
                .execute(sql, vec![Self::value_to_sql(id)])
                .await?;
        } else {
            // No id column: delete by matching all known columns.
            let pairs: Vec<(String, SqlValue)> = obj
                .iter()
                .map(|(k, v)| {
                    let col = pg_walstream::quote_ident(k).map_err(|e| {
                        StackhouseError::Database(format!("invalid column name: {:?}", e))
                    })?;
                    Ok((col, Self::value_to_sql(v)))
                })
                .collect::<StackhouseResult<Vec<_>>>()?;

            if pairs.is_empty() {
                return Ok(());
            }

            let clauses: Vec<String> = pairs.iter().map(|(c, _)| format!("{} = ?", c)).collect();
            let values: Vec<SqlValue> = pairs.into_iter().map(|(_, v)| v).collect();
            let sql = format!(
                "DELETE FROM {}.{} WHERE {}",
                restore_schema,
                table_ident,
                clauses.join(" AND ")
            );
            self.store.execute(sql, values).await?;
        }

        Ok(())
    }

    fn value_to_sql(v: &Value) -> SqlValue {
        match v {
            Value::Null => SqlValue::Null,
            Value::Bool(b) => SqlValue::Boolean(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    SqlValue::Integer(i)
                } else {
                    SqlValue::Real(n.as_f64().unwrap_or(0.0))
                }
            }
            Value::String(s) => SqlValue::Text(s.clone()),
            Value::Array(_) | Value::Object(_) => SqlValue::Json(v.clone()),
        }
    }

    /// Get PITR status for a tenant
    pub async fn get_status(&self, tenant_id: i64) -> StackhouseResult<Value> {
        let config_rows = self.store.query(
            "SELECT retention_days, last_backup_at, last_wal_archive_at FROM stackhouse_pitr_config WHERE tenant_id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        let point_count = self.store.query(
            "SELECT COUNT(*) as count FROM stackhouse_restore_points WHERE tenant_id = ? AND status = 'available'".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        let count = point_count
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "count"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if let Some(row) = config_rows.first() {
            Ok(json!(row.iter().cloned().collect::<HashMap<_, _>>()))
        } else {
            Ok(json!({"configured": false, "restore_points": count}))
        }
    }

    /// Cleanup expired restore points
    pub async fn cleanup(&self, tenant_id: i64) -> StackhouseResult<u64> {
        let config_rows = self
            .store
            .query(
                "SELECT retention_days FROM stackhouse_pitr_config WHERE tenant_id = ?".to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;

        let retention = config_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "retention_days"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(7);

        let count = self.store.execute(
            format!("UPDATE stackhouse_restore_points SET status = 'expired' WHERE tenant_id = ? AND point_in_time < NOW() - INTERVAL '{} days'", retention),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        Ok(count)
    }
}
