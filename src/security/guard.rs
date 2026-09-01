//! # Schema-Later Guard (Migration-Automaton)
//!
//! The "Heart" of Stackhouse. This module manages automatic schema evolution by:
//! 1. Caching known schemas using DashMap
//! 2. Detecting missing columns and type conflicts from incoming payloads
//! 3. Generating and executing safe `ALTER TABLE` statements
//! 4. Coordinating schema changes across instances with Postgres advisory locks
//!    and `LISTEN`/`NOTIFY` cache invalidation
//!
//! ## Execution Loop
//! For every write:
//! 1. **Cache Check**: Check DashMap for known table schema
//! 2. **Live Verify**: On cache miss or conflict, run `information_schema` query
//! 3. **Diffing**: Compare payload keys against existing columns and types
//! 4. **Auto-Migration**: Generate `ADD COLUMN` and `ALTER COLUMN TYPE ... USING ...`
//! 5. **Validation**: Ensure keys are valid SQL identifiers
//! 6. **Cross-Instance Coordination**: Wrap DDL in `pg_advisory_xact_lock` and
//!    broadcast `schema_changed` for cache eviction on every instance

use crate::db::schema_migrations::{Migration, SchemaMigrationService};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};
use crate::inference::{infer_batch_schema, infer_schema, InferredColumn, PgType};
use dashmap::DashMap;
use hex;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use sqlx::Row as _;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Maximum columns per table (prevents "Schema Bloat" attacks)
const MAX_COLUMNS_PER_TABLE: usize = 1000;

/// Default rolling window for schema churn rate limiting.
const DEFAULT_COLUMN_CHURN_WINDOW: Duration = Duration::from_secs(60);

/// Default number of new columns allowed per rolling window.
const DEFAULT_MAX_NEW_COLUMNS_PER_WINDOW: usize = 20;

lazy_static! {
    /// Regex for validating SQL identifiers
    /// Only alphanumeric characters and underscores, must start with letter or underscore
    static ref IDENTIFIER_REGEX: Regex = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();

    /// SQL reserved keywords that cannot be used as identifiers
    /// Full list from PostgreSQL reserved keywords (SQL:2016 + PG extensions)
    static ref RESERVED_KEYWORDS: HashSet<&'static str> = {
        let mut set = HashSet::new();
        // SQL standard reserved keywords
        for kw in &[
            "ALL", "ANALYSE", "ANALYZE", "AND", "ANY", "ARRAY", "AS", "ASC",
            "ASYMMETRIC", "AUTHORIZATION", "BINARY", "BOTH", "CASE", "CAST",
            "CHECK", "COLLATE", "COLLATION", "COLUMN", "CONCURRENTLY", "CONSTRAINT",
            "CREATE", "CROSS", "CURRENT_CATALOG", "CURRENT_DATE", "CURRENT_ROLE",
            "CURRENT_SCHEMA", "CURRENT_TIME", "CURRENT_TIMESTAMP", "CURRENT_USER",
            "DEFAULT", "DEFERRABLE", "DESC", "DISTINCT", "DO", "ELSE", "END",
            "EXCEPT", "FALSE", "FETCH", "FOR", "FOREIGN", "FREEZE", "FROM",
            "FULL", "GRANT", "GROUP", "HAVING", "ILIKE", "IN", "INITIALLY",
            "INNER", "INTERSECT", "INTO", "IS", "ISNULL", "JOIN", "LATERAL",
            "LEADING", "LEFT", "LIKE", "LIMIT", "LOCALTIME", "LOCALTIMESTAMP",
            "NATURAL", "NOT", "NOTNULL", "NULL", "OFFSET", "ON", "ONLY", "OR",
            "ORDER", "OUTER", "OVERLAPS", "PLACING", "PRIMARY", "REFERENCES",
            "RETURNING", "RIGHT", "SELECT", "SESSION_USER", "SIMILAR", "SOME",
            "SYMMETRIC", "TABLE", "TABLESAMPLE", "THEN", "TO", "TRAILING",
            "TRUE", "UNION", "UNIQUE", "USER", "USING", "VARIADIC", "VERBOSE",
            "WHEN", "WHERE", "WINDOW", "WITH",
            // Additional PostgreSQL reserved words
            "ALTER", "DROP", "INDEX", "KEY", "TRUNCATE", "INSERT", "UPDATE",
            "DELETE", "SET", "RESET", "SHOW", "COPY", "EXPLAIN", "VACUUM",
            "REINDEX", "CLUSTER", "COMMENT", "LISTEN", "NOTIFY", "UNLISTEN",
            "LOCK", "REVOKE", "ABORT", "BEGIN", "COMMIT", "ROLLBACK",
            "SAVEPOINT", "RELEASE", "PREPARE", "EXECUTE", "DEALLOCATE",
            "DECLARE", "FETCH", "CLOSE", "MERGE", "CALL", "DO",
            // Data types that are reserved
            "BIGINT", "BOOLEAN", "CHAR", "DECIMAL", "DOUBLE", "FLOAT", "INT",
            "INTEGER", "NUMERIC", "REAL", "SMALLINT", "TEXT", "VARCHAR",
            "DATE", "TIME", "TIMESTAMP", "INTERVAL", "BYTEA", "JSON", "JSONB",
            "UUID", "SERIAL", "BIGSERIAL", "MONEY",
        ] {
            set.insert(*kw);
        }
        set
    };
}

/// Column metadata stored in cache
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub col_type: String,
    pub notnull: bool,
    pub pk: bool,
}

/// Tracks how many new columns a table has gained within a rolling time window.
#[derive(Debug, Clone, Default)]
struct ChurnTracker {
    additions: Vec<Instant>,
}

impl ChurnTracker {
    fn new() -> Self {
        Self {
            additions: Vec::new(),
        }
    }

    fn prune(&mut self, window: Duration, now: Instant) {
        self.additions
            .retain(|t| now.saturating_duration_since(*t) <= window);
    }

    fn count(&self, window: Duration, now: Instant) -> usize {
        self.additions
            .iter()
            .filter(|t| now.saturating_duration_since(**t) <= window)
            .count()
    }

    fn record(&mut self, now: Instant, count: usize) {
        self.additions.extend(std::iter::repeat(now).take(count));
    }
}

/// Schema Guard - manages automatic schema evolution
pub struct SchemaGuard {
    /// Thread-safe schema cache: table_name -> Vec<column_names>
    schema_cache: Arc<DashMap<String, Vec<ColumnInfo>>>,
    /// Per-table locks used to serialize first-time table creation and schema
    /// changes within one process.
    table_locks: DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    /// Reference to the database store
    store: Arc<StackhouseStore>,
    /// Background LISTEN worker that evicts cache entries on `schema_changed`.
    listener_task: std::sync::OnceLock<tokio::task::JoinHandle<()>>,
    /// Per-table rolling-window tracker of new column additions.
    churn_tracker: Arc<DashMap<String, std::sync::Mutex<ChurnTracker>>>,
    /// Maximum new columns allowed per rolling window.
    max_new_columns_per_window: usize,
    /// Rolling window duration for churn rate limiting.
    churn_window: Duration,
    /// Lazily-initialized migration history service for automatic DDL.
    migration_service: tokio::sync::Mutex<Option<Arc<SchemaMigrationService>>>,
    /// Counter used to de-duplicate migration versions generated within the
    /// same nanosecond by this instance.
    migration_version_counter: AtomicU64,
}

impl SchemaGuard {
    /// Creates a new SchemaGuard with the given StackhouseStore
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        let max_new_columns_per_window = std::env::var("STACKHOUSE_SCHEMA_CHURN_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_NEW_COLUMNS_PER_WINDOW);

        let churn_window = std::env::var("STACKHOUSE_SCHEMA_CHURN_WINDOW_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_COLUMN_CHURN_WINDOW);

        Self {
            schema_cache: Arc::new(DashMap::new()),
            table_locks: DashMap::new(),
            store,
            listener_task: std::sync::OnceLock::new(),
            churn_tracker: Arc::new(DashMap::new()),
            max_new_columns_per_window,
            churn_window,
            migration_service: tokio::sync::Mutex::new(None),
            migration_version_counter: AtomicU64::new(0),
        }
    }

    /// Lazily initializes and returns the migration history service.
    async fn migration_service(&self) -> StackhouseResult<Arc<SchemaMigrationService>> {
        // Fast path: already initialized.
        if let Some(svc) = self
            .migration_service
            .try_lock()
            .ok()
            .and_then(|g| g.clone())
        {
            return Ok(svc);
        }

        // Initialization is serialized with a dedicated mutex so multiple
        // concurrent schema operations create only one history table.
        let mut guard = self.migration_service.lock().await;
        if let Some(svc) = guard.as_ref() {
            return Ok(Arc::clone(svc));
        }

        let svc = SchemaMigrationService::new(Arc::clone(&self.store)).await?;
        let arc = Arc::new(svc);
        *guard = Some(arc.clone());
        Ok(arc)
    }

    /// Generates a unique migration version for an automatic schema change.
    ///
    /// Combines the current Unix timestamp in nanoseconds with an instance-local
    /// monotonic counter. The counter only matters if multiple versions are
    /// generated within the same nanosecond.
    fn next_migration_version(&self) -> u64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let counter = self
            .migration_version_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        nanos.saturating_add(counter)
    }

    /// Records an automatic schema change in the migration history table.
    async fn record_auto_migration(
        &self,
        name: &str,
        up_sql: &str,
        down_sql: &str,
    ) -> StackhouseResult<()> {
        if up_sql.trim().is_empty() {
            return Ok(());
        }

        let svc = self.migration_service().await?;
        let version = self.next_migration_version();
        let id = format!("auto-{}-{}", version, uuid::Uuid::new_v4());
        let checksum = Self::compute_migration_checksum(up_sql);

        let migration = Migration {
            id,
            version,
            name: name.to_string(),
            up_sql: up_sql.to_string(),
            down_sql: down_sql.to_string(),
            checksum,
            applied_at: None,
            execution_time_ms: None,
            status: crate::db::schema_migrations::MigrationStatus::Applied,
        };

        svc.register(&migration).await?;
        Ok(())
    }

    fn compute_migration_checksum(sql: &str) -> String {
        let mut hasher = sha2::Sha256::new();
        hasher.update(sql.as_bytes());
        hex::encode(hasher.finalize())[..16].to_string()
    }

    /// Validates that an identifier is safe for use as a table/column name
    pub fn validate_identifier(name: &str) -> StackhouseResult<()> {
        if name.is_empty() || name.len() > 128 {
            return Err(StackhouseError::InvalidIdentifier(format!(
                "Identifier '{}' must be 1-128 characters",
                name
            )));
        }

        if !IDENTIFIER_REGEX.is_match(name) {
            return Err(StackhouseError::InvalidIdentifier(format!(
                "Identifier '{}' contains invalid characters. Use only alphanumeric and underscores, starting with a letter or underscore",
                name
            )));
        }

        if RESERVED_KEYWORDS.contains(name.to_uppercase().as_str()) {
            return Err(StackhouseError::InvalidIdentifier(format!(
                "Identifier '{}' is a SQL reserved keyword",
                name
            )));
        }

        Ok(())
    }

    /// Validates that a SQL expression is safe for use in RLS USING / WITH CHECK clauses.
    /// Rejects statement terminators, comments, and dangerous keywords.
    pub fn validate_sql_expression(expr: &str) -> StackhouseResult<()> {
        if expr.is_empty() || expr.len() > 2048 {
            return Err(StackhouseError::InvalidPayload(
                "RLS expression must be 1-2048 characters".to_string(),
            ));
        }

        let normalized = expr.to_lowercase();

        // Reject statement terminators and comments
        if normalized.contains(';')
            || normalized.contains("--")
            || normalized.contains("/*")
            || normalized.contains("*/")
        {
            return Err(StackhouseError::InvalidPayload(
                "RLS expression contains forbidden characters or comments".to_string(),
            ));
        }

        // Reject dangerous keywords that could modify data or schema
        let forbidden_keywords = [
            "delete", "drop", "insert", "update", "create", "alter", "grant", "revoke", "truncate",
            "execute", "exec", "union", "select", "copy", "listen", "notify", "load", "set",
            "reset", "show",
        ];
        for keyword in &forbidden_keywords {
            if let Some(pos) = normalized.find(keyword) {
                let before = normalized[..pos].chars().last();
                let after = normalized[pos + keyword.len()..].chars().next();
                let is_word_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
                let before_ok = before.map_or(true, |c| !is_word_char(c));
                let after_ok = after.map_or(true, |c| !is_word_char(c));
                if before_ok && after_ok {
                    return Err(StackhouseError::InvalidPayload(format!(
                        "RLS expression contains forbidden keyword '{}'",
                        keyword
                    )));
                }
            }
        }

        // Only allow printable ASCII characters commonly found in safe expressions
        if !expr.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    ' ' | '\t'
                        | '\n'
                        | '\r'
                        | '_'
                        | '.'
                        | ','
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '\''
                        | '"'
                        | '='
                        | '<'
                        | '>'
                        | '!'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '|'
                        | '&'
                        | '~'
                        | '#'
                        | ':'
                        | '?'
                        | '@'
                )
        }) {
            return Err(StackhouseError::InvalidPayload(
                "RLS expression contains invalid characters".to_string(),
            ));
        }

        Ok(())
    }

    /// Sanitizes a string to be a valid SQL identifier
    pub fn sanitize_identifier(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i == 0 {
                    if c.is_ascii_alphabetic() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                } else if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        sanitized.chars().take(128).collect()
    }

    /// Gets the current schema for a table from cache or database
    pub async fn get_table_schema(&self, table: &str) -> StackhouseResult<Vec<ColumnInfo>> {
        Self::validate_identifier(table)?;

        if let Some(cached) = self.schema_cache.get(table) {
            debug!("Schema cache hit for table: {}", table);
            return Ok(cached.clone());
        }

        debug!(
            "Schema cache miss for table: {}, querying Postgres schema",
            table
        );
        let columns = self.fetch_table_info(table).await?;

        if !columns.is_empty() {
            self.schema_cache.insert(table.to_string(), columns.clone());
        }

        Ok(columns)
    }

    /// Fetches table info using PostgreSQL information_schema
    async fn fetch_table_info(&self, table: &str) -> StackhouseResult<Vec<ColumnInfo>> {
        let sql = "SELECT column_name as name, data_type as type, 
                    is_nullable as nullable
             FROM information_schema.columns 
             WHERE table_name = $1 AND table_schema = current_schema()"
            .to_string();
        let rows = self
            .store
            .query(sql, vec![SqlValue::Text(table.to_string())])
            .await?;

        let mut columns = Vec::new();
        for row in rows {
            let name = row
                .iter()
                .find(|(k, _)| k == "name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or_default()
                .to_string();
            let col_type = row
                .iter()
                .find(|(k, _)| k == "type")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or_default()
                .to_string();
            let notnull = row
                .iter()
                .find(|(k, _)| k == "nullable")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("YES")
                != "YES";
            let pk = name == "id";

            if !name.is_empty() {
                columns.push(ColumnInfo {
                    name,
                    col_type,
                    notnull,
                    pk,
                });
            }
        }

        Ok(columns)
    }

    /// Fetches table info from inside an explicit transaction.
    async fn fetch_table_info_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        table: &str,
    ) -> StackhouseResult<Vec<ColumnInfo>> {
        let rows = sqlx::query(
            "SELECT column_name, data_type, is_nullable
             FROM information_schema.columns
             WHERE table_name = $1 AND table_schema = current_schema()",
        )
        .bind(table)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| StackhouseError::Database(format!("Failed to fetch table info: {}", e)))?;

        let mut columns = Vec::with_capacity(rows.len());
        for row in rows {
            let name: String = row.try_get("column_name").unwrap_or_default();
            let col_type: String = row.try_get("data_type").unwrap_or_default();
            let notnull: bool = row
                .try_get::<String, _>("is_nullable")
                .unwrap_or("YES".to_string())
                != "YES";
            let pk = name == "id";

            if !name.is_empty() {
                columns.push(ColumnInfo {
                    name,
                    col_type,
                    notnull,
                    pk,
                });
            }
        }

        Ok(columns)
    }

    /// Ensures a table exists with the base schema
    pub async fn ensure_table(&self, table: &str) -> StackhouseResult<()> {
        Self::validate_identifier(table)?;
        self.init_cache_invalidation().await;

        if let Some(cached) = self.schema_cache.get(table) {
            if !cached.is_empty() {
                return Ok(());
            }
        }

        let table_lock = self
            .table_locks
            .entry(table.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _table_lock = table_lock.lock().await;

        // Double-check live before taking an advisory lock; prevents most
        // unnecessary transaction overhead.
        let live = self.fetch_table_info(table).await?;
        if !live.is_empty() {
            self.schema_cache.insert(table.to_string(), live);
            return Ok(());
        }

        let mut tx = self.store.pool().begin().await.map_err(|e| {
            StackhouseError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        // Use a namespaced hash for the advisory lock so we don’t collide with
        // other code that uses the bare table name.
        sqlx::query("SELECT pg_advisory_xact_lock((hashtext('stackhouse:schema:' || $1))::bigint)")
            .bind(table)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                StackhouseError::Database(format!("Failed to acquire advisory lock: {}", e))
            })?;

        let live = Self::fetch_table_info_tx(&mut tx, table).await?;
        if !live.is_empty() {
            tx.rollback()
                .await
                .map_err(|e| StackhouseError::Database(format!("Rollback failed: {}", e)))?;
            self.schema_cache.insert(table.to_string(), live);
            return Ok(());
        }

        // Create table with Postgres BIGSERIAL for auto-increment
        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id BIGSERIAL PRIMARY KEY,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
            )",
            table
        );

        sqlx::query(&create_sql)
            .execute(&mut *tx)
            .await
            .map_err(|e| StackhouseError::Database(format!("Failed to create table: {}", e)))?;

        sqlx::query("SELECT pg_notify('schema_changed', $1)")
            .bind(table)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                StackhouseError::Database(format!("Failed to notify schema change: {}", e))
            })?;

        tx.commit().await.map_err(|e| {
            StackhouseError::Database(format!("Failed to commit create table: {}", e))
        })?;

        let _ = self
            .record_auto_migration(
                &format!("create_table_{}", table),
                &create_sql,
                &format!("DROP TABLE IF EXISTS {}", table),
            )
            .await;

        info!("✨ Created table: {}", table);
        self.schema_cache.remove(table);

        Ok(())
    }

    /// Ensures all columns from a single payload exist in the table and are wide
    /// enough to hold the payload values. Returns the insertable columns with
    /// their target types so the caller can bind parameters correctly.
    pub async fn ensure_columns(
        &self,
        table: &str,
        payload: &Value,
    ) -> StackhouseResult<Vec<(String, PgType)>> {
        let inferred = infer_schema(payload)?;
        self.ensure_schema(table, &inferred).await
    }

    /// Ensures a unified batch schema exists in the table and returns the union
    /// of insertable columns with their target types.
    pub async fn ensure_batch_columns(
        &self,
        table: &str,
        payloads: &[Value],
    ) -> StackhouseResult<Vec<(String, PgType)>> {
        let inferred = infer_batch_schema(payloads)?;
        self.ensure_schema(table, &inferred).await
    }

    /// Core schema-evolution routine. Computes the diff between the desired
    /// `inferred` columns and the live table schema, acquires a cross-instance
    /// advisory lock, applies all `ADD COLUMN`/`ALTER COLUMN TYPE` changes in a
    /// single transaction, and invalidates the local cache.
    pub async fn ensure_schema(
        &self,
        table: &str,
        inferred: &[InferredColumn],
    ) -> StackhouseResult<Vec<(String, PgType)>> {
        Self::validate_identifier(table)?;
        for col in inferred {
            Self::validate_identifier(&col.name)?;
        }
        self.init_cache_invalidation().await;

        // Short-circuit using the in-process cache. If the cache says the live
        // schema can already hold every value, we can skip the DB round-trip.
        if let Some(cached) = self.schema_cache.get(table) {
            if let Ok(result) = self.schema_from_cache(table, inferred, &cached) {
                return Ok(result);
            }
        }

        let table_lock = self
            .table_locks
            .entry(table.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _table_lock = table_lock.lock().await;

        let mut tx = self.store.pool().begin().await.map_err(|e| {
            StackhouseError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        // Use a namespaced hash for the advisory lock so we don’t collide with
        // other code that uses the bare table name.
        sqlx::query("SELECT pg_advisory_xact_lock((hashtext('stackhouse:schema:' || $1))::bigint)")
            .bind(table)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                StackhouseError::Database(format!("Failed to acquire advisory lock: {}", e))
            })?;

        let live = Self::fetch_table_info_tx(&mut tx, table).await?;
        let live_by_lower: HashMap<String, &ColumnInfo> =
            live.iter().map(|c| (c.name.to_lowercase(), c)).collect();

        let mut adds: Vec<(String, PgType)> = Vec::new();
        let mut widens: Vec<(String, PgType, PgType)> = Vec::new();
        let mut result: Vec<(String, PgType)> = Vec::with_capacity(inferred.len());

        for col in inferred {
            let key_lower = col.name.to_lowercase();
            if key_lower == "id" || key_lower == "created_at" || key_lower == "updated_at" {
                continue;
            }

            if let Some(info) = live_by_lower.get(&key_lower) {
                let existing = PgType::from_data_type(&info.col_type).ok_or_else(|| {
                    StackhouseError::Schema(format!(
                        "Unknown existing column type '{}' for {}.{}",
                        info.col_type, table, info.name
                    ))
                })?;
                let common = PgType::common_type(&existing, &col.pg_type);

                if common != existing {
                    if !existing.can_promote_to(&common) {
                        return Err(StackhouseError::Schema(format!(
                            "Cannot promote column {}.{} from {} to {}",
                            table,
                            info.name,
                            existing.as_sql(),
                            common.as_sql()
                        )));
                    }
                    widens.push((info.name.clone(), existing, common.clone()));
                }

                result.push((col.name.clone(), common));
            } else {
                adds.push((col.name.clone(), col.pg_type.clone()));
                result.push((col.name.clone(), col.pg_type.clone()));
            }
        }

        // Hard column count cap
        let existing_count = live_by_lower.len();
        if existing_count + adds.len() > MAX_COLUMNS_PER_TABLE {
            return Err(StackhouseError::ColumnLimitExceeded {
                message: format!(
                    "Table '{}' would exceed {} column limit ({} existing + {} new = {})",
                    table,
                    MAX_COLUMNS_PER_TABLE,
                    existing_count,
                    adds.len(),
                    existing_count + adds.len()
                ),
            });
        }

        // Schema churn rate limit (in-process; per-instance unless shared store is added)
        if !adds.is_empty() {
            let tracker = self
                .churn_tracker
                .entry(table.to_string())
                .or_insert_with(|| std::sync::Mutex::new(ChurnTracker::new()));
            let mut guard = tracker.lock().map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Churn tracker poisoned: {}", e))
            })?;
            let now = Instant::now();
            guard.prune(self.churn_window, now);
            let churn_count = guard.count(self.churn_window, now);
            if churn_count + adds.len() > self.max_new_columns_per_window {
                return Err(StackhouseError::RateLimited(format!(
                    "Table '{}' has added {} new columns in the last {:?}; limit is {} per rolling window. This is a schema churn rate limit, distinct from the hard {}-column cap.",
                    table,
                    churn_count,
                    self.churn_window,
                    self.max_new_columns_per_window,
                    MAX_COLUMNS_PER_TABLE
                )));
            }
        }

        // Idempotent re-check: if another instance already performed the same
        // widening, skip the redundant `ALTER`.
        let mut final_widens: Vec<(String, PgType, PgType)> = Vec::new();
        for (name, from, to) in widens {
            if let Some(info) = live_by_lower.get(&name.to_lowercase()) {
                let current = PgType::from_data_type(&info.col_type).unwrap_or(from.clone());
                let common = PgType::common_type(&current, &to);
                if common == current {
                    // Already at target or at an even wider type.
                    continue;
                }
            }
            final_widens.push((name, from, to));
        }

        // Apply type widenings
        let mut widen_sql = String::new();
        if !final_widens.is_empty() {
            let widen_parts: Vec<String> = final_widens
                .iter()
                .map(|(name, from, to)| {
                    let using = to.using_cast_expr(from, name);
                    format!("ALTER COLUMN {} TYPE {} USING {}", name, to.as_sql(), using)
                })
                .collect();

            widen_sql = format!("ALTER TABLE {} {}", table, widen_parts.join(", "));
            sqlx::query(&widen_sql)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    StackhouseError::Database(format!("Failed to widen columns: {}", e))
                })?;

            for (name, from, to) in &final_widens {
                info!(
                    "🔄 Widened column: {}.{} ({} -> {})",
                    table,
                    name,
                    from.as_sql(),
                    to.as_sql()
                );
            }
        }

        // Add new columns
        let mut add_sql = String::new();
        if !adds.is_empty() {
            let add_parts: Vec<String> = adds
                .iter()
                .map(|(name, pg_type)| {
                    format!("ADD COLUMN IF NOT EXISTS {} {}", name, pg_type.as_sql())
                })
                .collect();

            add_sql = format!("ALTER TABLE {} {}", table, add_parts.join(", "));
            sqlx::query(&add_sql)
                .execute(&mut *tx)
                .await
                .map_err(|e| StackhouseError::Database(format!("Failed to add columns: {}", e)))?;

            for (name, pg_type) in &adds {
                info!("📊 Added column: {}.{} ({})", table, name, pg_type.as_sql());
            }
        }

        sqlx::query("SELECT pg_notify('schema_changed', $1)")
            .bind(table)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                StackhouseError::Database(format!("Failed to notify schema change: {}", e))
            })?;

        tx.commit().await.map_err(|e| {
            StackhouseError::Database(format!("Failed to commit schema changes: {}", e))
        })?;

        // Record churn for the new columns now that the transaction succeeded.
        if !adds.is_empty() {
            let tracker = self
                .churn_tracker
                .entry(table.to_string())
                .or_insert_with(|| std::sync::Mutex::new(ChurnTracker::new()));
            let mut guard = tracker.lock().map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Churn tracker poisoned: {}", e))
            })?;
            guard.record(Instant::now(), adds.len());
        }

        // Record the automatic schema migration in the audit trail.
        let up_sql = [widen_sql, add_sql]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        if !up_sql.is_empty() {
            let down_parts: Vec<String> = final_widens
                .iter()
                .map(|(name, from, to)| {
                    let using = from.using_cast_expr(to, name);
                    format!(
                        "ALTER COLUMN {} TYPE {} USING {}",
                        name,
                        from.as_sql(),
                        using
                    )
                })
                .chain(
                    adds.iter()
                        .map(|(name, _)| format!("DROP COLUMN IF EXISTS {}", name)),
                )
                .collect();
            let down_sql = if down_parts.is_empty() {
                String::new()
            } else {
                format!("ALTER TABLE {} {}", table, down_parts.join(", "))
            };

            let _ = self
                .record_auto_migration(&format!("evolve_schema_{}", table), &up_sql, &down_sql)
                .await;
        }

        self.schema_cache.remove(table);

        Ok(result)
    }

    /// Attempts to satisfy a schema request entirely from the in-process cache.
    /// Returns `Ok` only when the cached schema can already hold every value.
    fn schema_from_cache(
        &self,
        table: &str,
        inferred: &[InferredColumn],
        cached: &[ColumnInfo],
    ) -> StackhouseResult<Vec<(String, PgType)>> {
        let mut result = Vec::with_capacity(inferred.len());
        let by_lower: HashMap<String, &ColumnInfo> =
            cached.iter().map(|c| (c.name.to_lowercase(), c)).collect();

        for col in inferred {
            let key_lower = col.name.to_lowercase();
            if key_lower == "id" || key_lower == "created_at" || key_lower == "updated_at" {
                continue;
            }

            if let Some(info) = by_lower.get(&key_lower) {
                let existing = PgType::from_data_type(&info.col_type).ok_or_else(|| {
                    StackhouseError::Schema(format!(
                        "Unknown existing column type '{}' for {}.{}",
                        info.col_type, table, info.name
                    ))
                })?;
                let common = PgType::common_type(&existing, &col.pg_type);

                // Cache is only trustworthy if the live type is already the
                // common supertype; otherwise we need a live verify.
                if common != existing {
                    return Err(StackhouseError::Schema(
                        "Cache stale, requires live verify".to_string(),
                    ));
                }

                result.push((col.name.clone(), common));
            } else {
                return Err(StackhouseError::Schema(
                    "Cache missing column, requires live verify".to_string(),
                ));
            }
        }

        Ok(result)
    }

    /// Computes the DDL that `ensure_columns`/`ensure_batch_columns` would run
    /// for a sample payload without executing it. Returns the additions,
    /// widenings, and the insertable column list.
    pub async fn preview_schema_changes(
        &self,
        table: &str,
        payload: &Value,
    ) -> StackhouseResult<SchemaPreview> {
        let inferred = if let Some(array) = payload.as_array() {
            infer_batch_schema(array)?
        } else {
            infer_schema(payload)?
        };
        self.preview_schema(table, &inferred).await
    }

    async fn preview_schema(
        &self,
        table: &str,
        inferred: &[InferredColumn],
    ) -> StackhouseResult<SchemaPreview> {
        Self::validate_identifier(table)?;
        for col in inferred {
            Self::validate_identifier(&col.name)?;
        }

        let table_exists = !self.get_table_schema(table).await?.is_empty();

        let mut create_table_sql = None;
        if !table_exists {
            create_table_sql = Some(format!(
                "CREATE TABLE IF NOT EXISTS {} (
                    id BIGSERIAL PRIMARY KEY,
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
                )",
                table
            ));
        }

        let live = self.fetch_table_info(table).await.unwrap_or_default();
        let live_by_lower: HashMap<String, &ColumnInfo> =
            live.iter().map(|c| (c.name.to_lowercase(), c)).collect();

        let mut additions: Vec<(String, PgType)> = Vec::new();
        let mut widenings: Vec<(String, PgType, PgType, String)> = Vec::new();
        let mut insert_columns: Vec<(String, PgType)> = Vec::with_capacity(inferred.len());

        for col in inferred {
            let key_lower = col.name.to_lowercase();
            if key_lower == "id" || key_lower == "created_at" || key_lower == "updated_at" {
                continue;
            }

            if let Some(info) = live_by_lower.get(&key_lower) {
                if let Some(existing) = PgType::from_data_type(&info.col_type) {
                    let common = PgType::common_type(&existing, &col.pg_type);
                    if common != existing {
                        if existing.can_promote_to(&common) {
                            let using = common.using_cast_expr(&existing, &info.name);
                            widenings.push((info.name.clone(), existing, common.clone(), using));
                        }
                    }
                    insert_columns.push((col.name.clone(), common));
                } else {
                    insert_columns.push((col.name.clone(), col.pg_type.clone()));
                }
            } else {
                additions.push((col.name.clone(), col.pg_type.clone()));
                insert_columns.push((col.name.clone(), col.pg_type.clone()));
            }
        }

        let mut add_sql = Vec::new();
        if !additions.is_empty() {
            let parts: Vec<String> = additions
                .iter()
                .map(|(name, pg_type)| {
                    format!("ADD COLUMN IF NOT EXISTS {} {}", name, pg_type.as_sql())
                })
                .collect();
            add_sql.push(format!("ALTER TABLE {} {}", table, parts.join(", ")));
        }

        let mut widen_sql = Vec::new();
        if !widenings.is_empty() {
            let parts: Vec<String> = widenings
                .iter()
                .map(|(name, _from, to, using)| {
                    format!("ALTER COLUMN {} TYPE {} USING {}", name, to.as_sql(), using)
                })
                .collect();
            widen_sql.push(format!("ALTER TABLE {} {}", table, parts.join(", ")));
        }

        let existing_count = live_by_lower.len();
        let would_exceed_limit = existing_count + additions.len() > MAX_COLUMNS_PER_TABLE;

        Ok(SchemaPreview {
            table_exists,
            create_table_sql,
            additions,
            widenings: widenings
                .into_iter()
                .map(|(name, from, to, _)| (name, from, to))
                .collect(),
            add_sql,
            widen_sql,
            insert_columns,
            would_exceed_limit,
        })
    }

    /// Spawns a background `LISTEN schema_changed` worker on the first schema
    /// operation. This keeps per-instance caches eventually consistent across
    /// horizontally-scaled Stackhouse instances.
    async fn init_cache_invalidation(&self) {
        if self.listener_task.get().is_some() {
            return;
        }

        let store = Arc::clone(&self.store);
        let cache = Arc::clone(&self.schema_cache);

        let handle = tokio::spawn(async move {
            Self::cache_invalidation_worker(store, cache).await;
        });

        let _ = self.listener_task.set(handle);
    }

    async fn cache_invalidation_worker(
        store: Arc<StackhouseStore>,
        cache: Arc<DashMap<String, Vec<ColumnInfo>>>,
    ) {
        let pool = store.pool();
        let mut listener = match sqlx::postgres::PgListener::connect_with(pool).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("Failed to connect PgListener for schema cache: {}", e);
                return;
            }
        };

        if let Err(e) = listener.listen("schema_changed").await {
            tracing::warn!("Failed to LISTEN on schema_changed: {}", e);
            return;
        }

        loop {
            match listener.recv().await {
                Ok(notification) => {
                    let table = notification.payload().trim();
                    if !table.is_empty() {
                        cache.remove(table);
                        debug!("Evicted schema cache for table: {}", table);
                    }
                }
                Err(e) => {
                    tracing::warn!("PgListener recv error: {}", e);
                    break;
                }
            }
        }
    }

    /// Adds new columns to a table (kept for callers that do their own
    /// coordination; `ensure_schema` should be preferred).
    pub(crate) async fn add_columns(
        &self,
        table: &str,
        columns: &[(String, PgType)],
    ) -> StackhouseResult<()> {
        if columns.is_empty() {
            return Ok(());
        }

        let parts: Vec<String> = columns
            .iter()
            .map(|(name, pg_type)| {
                format!("ADD COLUMN IF NOT EXISTS {} {}", name, pg_type.as_sql())
            })
            .collect();
        let sql = format!("ALTER TABLE {} {}", table, parts.join(", "));

        self.store
            .execute_simple(sql)
            .await
            .map_err(|e| StackhouseError::Database(format!("Failed to add columns: {}", e)))?;

        for (name, pg_type) in columns {
            info!("📊 Added column: {}.{} ({})", table, name, pg_type.as_sql());
        }

        self.schema_cache.remove(table);
        Ok(())
    }

    /// Widens existing columns (kept for callers that do their own
    /// coordination; `ensure_schema` should be preferred).
    pub(crate) async fn widen_columns(
        &self,
        table: &str,
        widenings: &[(String, PgType, PgType)],
    ) -> StackhouseResult<()> {
        if widenings.is_empty() {
            return Ok(());
        }

        let parts: Vec<String> = widenings
            .iter()
            .map(|(name, from, to)| {
                let using = to.using_cast_expr(from, name);
                format!("ALTER COLUMN {} TYPE {} USING {}", name, to.as_sql(), using)
            })
            .collect();
        let sql = format!("ALTER TABLE {} {}", table, parts.join(", "));

        self.store
            .execute_simple(sql)
            .await
            .map_err(|e| StackhouseError::Database(format!("Failed to widen columns: {}", e)))?;

        for (name, from, to) in widenings {
            info!(
                "🔄 Widened column: {}.{} ({} -> {})",
                table,
                name,
                from.as_sql(),
                to.as_sql()
            );
        }

        self.schema_cache.remove(table);
        Ok(())
    }

    /// Gets table statistics
    pub async fn get_table_stats(&self, table: &str) -> StackhouseResult<TableStats> {
        let schema = self.get_table_schema(table).await?;

        if schema.is_empty() {
            return Err(StackhouseError::TableNotFound(table.to_string()));
        }

        let sql = format!("SELECT COUNT(*) as count FROM {}", table);
        let rows = self.store.query_simple(sql).await?;
        let row_count: i64 = rows
            .first()
            .and_then(|r| r.first())
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        Ok(TableStats {
            name: table.to_string(),
            column_count: schema.len(),
            row_count: row_count as u64,
            columns: schema,
        })
    }

    pub fn clear_cache(&self) {
        self.schema_cache.clear();
    }

    pub fn cached_tables(&self) -> Vec<String> {
        self.schema_cache.iter().map(|r| r.key().clone()).collect()
    }
}

/// Result of a dry-run schema preview.
#[derive(Debug, Clone, Serialize)]
pub struct SchemaPreview {
    pub table_exists: bool,
    pub create_table_sql: Option<String>,
    pub additions: Vec<(String, PgType)>,
    pub widenings: Vec<(String, PgType, PgType)>,
    pub add_sql: Vec<String>,
    pub widen_sql: Vec<String>,
    pub insert_columns: Vec<(String, PgType)>,
    pub would_exceed_limit: bool,
}

#[derive(Debug, Clone)]
pub struct TableStats {
    pub name: String,
    pub column_count: usize,
    pub row_count: u64,
    pub columns: Vec<ColumnInfo>,
}
