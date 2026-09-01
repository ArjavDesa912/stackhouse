//! # Stackhouse CLI
//!
//! A local development and management CLI for Stackhouse.
//!
//! Supported commands:
//! - `stackhouse serve` — start the API server
//! - `stackhouse init` — initialize a new Stackhouse project
//! - `stackhouse db push <collection> <file>` — push JSON records to a collection
//! - `stackhouse db diff` — show schema drift
//! - `stackhouse db query <sql>` — run a SQL query
//! - `stackhouse functions deploy <file>` — deploy an edge function
//! - `stackhouse gen types` — generate TypeScript types
//! - `stackhouse status` — check server status
//! - `stackhouse backup` — create a database backup
//! - `stackhouse branch <subcommand>` — manage database branches
//! - `stackhouse fixtures load <file>` — load JSON fixtures

pub mod fixtures;
pub mod quickstarts;
pub mod typescript_types;

pub use fixtures::*;
pub use quickstarts::*;
pub use typescript_types::*;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::Value;
use tracing::{error, info};

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

/// Stackhouse CLI — local dev & management
#[derive(Parser, Debug)]
#[command(name = "stackhouse")]
#[command(about = "Stackhouse — Schema-Later Database")]
#[command(version = "1.0.0")]
pub struct Cli {
    /// Database connection string
    #[arg(short, long, global = true, env = "STACKHOUSE_URL")]
    pub url: Option<String>,

    /// Server base URL
    #[arg(short, long, global = true, env = "STACKHOUSE_BASE_URL")]
    pub base_url: Option<String>,

    /// Service role/API key
    #[arg(long, global = true, env = "STACKHOUSE_SERVICE_KEY")]
    pub service_key: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the API server
    Serve(ServeArgs),

    /// Initialize a new Stackhouse project
    Init(InitArgs),

    /// Database operations
    #[command(subcommand)]
    Db(DbCommand),

    /// Edge function operations
    #[command(subcommand)]
    Functions(FunctionsCommand),

    /// Generate code/types
    #[command(subcommand)]
    Gen(GenCommand),

    /// Check server status
    Status,

    /// Backup operations
    #[command(subcommand)]
    Backup(BackupCommand),

    /// Branch operations
    #[command(subcommand)]
    Branch(BranchCommand),

    /// Load fixtures / seed data
    #[command(subcommand)]
    Fixtures(FixturesCommand),
}

#[derive(Parser, Debug, Clone)]
pub struct ServeArgs {
    /// Port to bind to
    #[arg(short, long, env = "STACKHOUSE_PORT", default_value = "3000")]
    pub port: u16,

    /// Host to bind to
    #[arg(long, env = "STACKHOUSE_HOST", default_value = "0.0.0.0")]
    pub host: String,

    /// Use an in-memory test database
    #[arg(short, long, env = "STACKHOUSE_MEMORY")]
    pub memory: bool,

    /// JWT secret (required for server mode)
    #[arg(long, env = "STACKHOUSE_JWT_SECRET")]
    pub jwt_secret: Option<String>,

    /// Storage path
    #[arg(long, env = "STACKHOUSE_STORAGE_PATH")]
    pub storage_path: Option<String>,
}

#[derive(Parser, Debug)]
pub struct InitArgs {
    /// Project name
    #[arg(default_value = "my-stackhouse-project")]
    pub name: String,

    /// Directory to create the project in
    #[arg(short, long)]
    pub dir: Option<PathBuf>,

    /// Initialize with TypeScript client
    #[arg(long)]
    pub ts: bool,

    /// Initialize with Python client
    #[arg(long)]
    pub python: bool,
}

#[derive(Subcommand, Debug)]
pub enum DbCommand {
    /// Push records from a JSON/JSONL file into a collection
    Push(PushArgs),
    /// Show schema diff / drift
    Diff(DiffArgs),
    /// Run a SQL query
    Query(QueryArgs),
}

#[derive(Parser, Debug)]
pub struct PushArgs {
    /// Target collection/table
    pub collection: String,
    /// JSON or JSONL file to push
    pub file: PathBuf,
    /// Upsert by key
    #[arg(short, long)]
    pub on_conflict: Option<String>,
}

#[derive(Parser, Debug)]
pub struct DiffArgs {
    /// Compare two branches
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub target: Option<String>,
}

#[derive(Parser, Debug)]
pub struct QueryArgs {
    /// SQL query to run
    pub query: String,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum FunctionsCommand {
    /// Deploy an edge function
    Deploy(FunctionDeployArgs),
    /// List deployed functions
    List,
    /// Delete a function
    Delete { name: String },
}

#[derive(Parser, Debug)]
pub struct FunctionDeployArgs {
    /// Function source file (JavaScript/TypeScript)
    pub file: PathBuf,
    /// Function name
    #[arg(short, long)]
    pub name: Option<String>,
    /// HTTP route/slug
    #[arg(short, long)]
    pub slug: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum GenCommand {
    /// Generate TypeScript types from schema
    Types(TypesArgs),
}

#[derive(Parser, Debug)]
pub struct TypesArgs {
    /// Output file
    #[arg(short, long, default_value = "stackhouse.types.ts")]
    pub output: PathBuf,
    /// Print to stdout instead of writing
    #[arg(long)]
    pub stdout: bool,
}

#[derive(Subcommand, Debug)]
pub enum BackupCommand {
    /// Create a full backup marker in PITR config
    Create,
    /// List available restore points
    List,
}

#[derive(Subcommand, Debug)]
pub enum BranchCommand {
    /// List branches
    List,
    /// Create a new branch
    Create {
        name: String,
        parent: Option<String>,
    },
    /// Reset a branch
    Reset { name: String },
    /// Delete a branch
    Delete { name: String },
}

#[derive(Subcommand, Debug)]
pub enum FixturesCommand {
    /// Load fixtures from a JSON file
    Load { file: PathBuf },
    /// Generate a standard test fixture file
    Seed { file: PathBuf },
}

/// CLI dispatcher
pub struct CliRunner;

impl CliRunner {
    /// Run a CLI command against a StackhouseStore
    pub async fn run(
        _cli: &Cli,
        cmd: &Command,
        store: std::sync::Arc<StackhouseStore>,
    ) -> StackhouseResult<()> {
        match cmd {
            Command::Db(DbCommand::Push(args)) => Self::db_push(store, args).await,
            Command::Db(DbCommand::Diff(args)) => Self::db_diff(store, args).await,
            Command::Db(DbCommand::Query(args)) => Self::db_query(store, args).await,
            Command::Functions(FunctionsCommand::Deploy(args)) => {
                Self::function_deploy(store, args).await
            }
            Command::Functions(FunctionsCommand::List) => Self::function_list(store).await,
            Command::Functions(FunctionsCommand::Delete { name }) => {
                Self::function_delete(store, name).await
            }
            Command::Gen(GenCommand::Types(args)) => Self::gen_types(store, args).await,
            Command::Status => Self::status(store).await,
            Command::Backup(BackupCommand::Create) => Self::backup_create(store).await,
            Command::Backup(BackupCommand::List) => Self::backup_list(store).await,
            Command::Branch(BranchCommand::List) => Self::branch_list(store).await,
            Command::Branch(BranchCommand::Create { name, parent }) => {
                Self::branch_create(store, name, parent.as_deref()).await
            }
            Command::Branch(BranchCommand::Reset { name }) => Self::branch_reset(store, name).await,
            Command::Branch(BranchCommand::Delete { name }) => {
                Self::branch_delete(store, name).await
            }
            Command::Fixtures(FixturesCommand::Load { file }) => {
                Self::fixtures_load(store, file).await
            }
            Command::Fixtures(FixturesCommand::Seed { file }) => Self::fixtures_seed(file).await,
            Command::Init(_args) => {
                // Init is handled before DB store is built
                Self::init_project(_args).await
            }
            Command::Serve(_) => {
                // Serve is handled in main.rs directly
                Ok(())
            }
        }
    }

    async fn db_push(
        store: std::sync::Arc<StackhouseStore>,
        args: &PushArgs,
    ) -> StackhouseResult<()> {
        info!(
            "🚀 Pushing {} into collection '{}'",
            args.file.display(),
            args.collection
        );

        let raw = tokio::fs::read_to_string(&args.file)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Read file: {}", e)))?;

        let records: Vec<Value> = if raw.trim().starts_with('[') {
            serde_json::from_str(&raw)
                .map_err(|e| StackhouseError::InvalidPayload(format!("JSON parse: {}", e)))?
        } else {
            let mut list = Vec::new();
            for line in raw.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                let v: Value = serde_json::from_str(line)
                    .map_err(|e| StackhouseError::InvalidPayload(format!("JSONL parse: {}", e)))?;
                list.push(v);
            }
            list
        };

        let table = format!("stackhouse_{}", args.collection.trim());
        let mut inserted = 0u64;
        for record in records {
            if let Some(obj) = record.as_object() {
                let columns: Vec<String> = obj.keys().cloned().collect();
                let values: Vec<SqlValue> = columns
                    .iter()
                    .map(|col| {
                        SqlValue::Text(obj.get(col).unwrap_or(&serde_json::json!(null)).to_string())
                    })
                    .collect();
                let placeholders: Vec<String> = columns.iter().map(|_| "?".to_string()).collect();

                let sql = if let Some(conflict) = &args.on_conflict {
                    format!(
                        "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT ({}) DO UPDATE SET {}",
                        table,
                        columns.join(", "),
                        placeholders.join(", "),
                        conflict,
                        columns
                            .iter()
                            .map(|c| format!("{} = EXCLUDED.{}", c, c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                } else {
                    format!(
                        "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT DO NOTHING",
                        table,
                        columns.join(", "),
                        placeholders.join(", ")
                    )
                };

                match store.execute(sql, values).await {
                    Ok(n) => inserted += n,
                    Err(e) => error!("Push failed for record: {}", e),
                }
            }
        }

        info!("✅ Pushed {} records into '{}'", inserted, table);
        Ok(())
    }

    async fn db_diff(
        store: std::sync::Arc<StackhouseStore>,
        args: &DiffArgs,
    ) -> StackhouseResult<()> {
        let source = args.source.as_deref().unwrap_or("public");
        let target = args.target.as_deref().unwrap_or("public");

        let source_tables = store.query(
            format!("SELECT table_name FROM information_schema.tables WHERE table_schema = '{}' AND table_type = 'BASE TABLE' ORDER BY table_name", source),
            vec![],
        ).await?;

        let target_tables = if source == target {
            store.query(
                "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY table_name".to_string(),
                vec![],
            ).await?
        } else {
            store.query(
                format!("SELECT table_name FROM information_schema.tables WHERE table_schema = '{}' AND table_type = 'BASE TABLE' ORDER BY table_name", target),
                vec![],
            ).await?
        };

        let s_names: std::collections::HashSet<_> = source_tables
            .iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "table_name")
                    .and_then(|(_, v)| v.as_str())
                    .map(String::from)
            })
            .collect();
        let t_names: std::collections::HashSet<_> = target_tables
            .iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "table_name")
                    .and_then(|(_, v)| v.as_str())
                    .map(String::from)
            })
            .collect();

        let only_source: Vec<_> = s_names.difference(&t_names).collect();
        let only_target: Vec<_> = t_names.difference(&s_names).collect();

        if only_source.is_empty() && only_target.is_empty() {
            info!("✅ Schemas are identical");
        } else {
            for t in &only_source {
                info!("- only in {}: {}", source, t);
            }
            for t in &only_target {
                info!("+ only in {}: {}", target, t);
            }
        }
        Ok(())
    }

    async fn db_query(
        store: std::sync::Arc<StackhouseStore>,
        args: &QueryArgs,
    ) -> StackhouseResult<()> {
        let rows = store.query(args.query.clone(), vec![]).await?;
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&rows).unwrap_or_default()
            );
        } else {
            for row in rows {
                let map: std::collections::HashMap<_, _> = row.into_iter().collect();
                println!("{:?}", map);
            }
        }
        Ok(())
    }

    async fn function_deploy(
        store: std::sync::Arc<StackhouseStore>,
        args: &FunctionDeployArgs,
    ) -> StackhouseResult<()> {
        let source = tokio::fs::read_to_string(&args.file)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Read function file: {}", e)))?;

        let name = args
            .name
            .as_ref()
            .cloned()
            .or_else(|| {
                args.file
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "unnamed".to_string());

        let slug = args.slug.as_ref().cloned().unwrap_or_else(|| name.clone());
        let id = uuid::Uuid::new_v4().to_string();

        info!("✅ Function '{}' deployed with slug '/{}'", name, slug);

        store.execute(
            "INSERT INTO stackhouse_functions (id, name, slug, source_code, runtime, status) VALUES (?, ?, ?, ?, 'javascript', 'active') ON CONFLICT (name) DO UPDATE SET source_code = EXCLUDED.source_code, slug = EXCLUDED.slug, status = 'active'".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Text(name),
                SqlValue::Text(slug),
                SqlValue::Text(source),
            ],
        ).await?;

        Ok(())
    }

    async fn function_list(store: std::sync::Arc<StackhouseStore>) -> StackhouseResult<()> {
        let rows = store
            .query(
                "SELECT name, slug, runtime, status FROM stackhouse_functions ORDER BY name"
                    .to_string(),
                vec![],
            )
            .await?;

        for row in rows {
            let name = row
                .iter()
                .find(|(k, _)| k == "name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let slug = row
                .iter()
                .find(|(k, _)| k == "slug")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let runtime = row
                .iter()
                .find(|(k, _)| k == "runtime")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let status = row
                .iter()
                .find(|(k, _)| k == "status")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            println!("{}\t{}\t{}\t{}", name, slug, runtime, status);
        }
        Ok(())
    }

    async fn function_delete(
        store: std::sync::Arc<StackhouseStore>,
        name: &str,
    ) -> StackhouseResult<()> {
        store
            .execute(
                "DELETE FROM stackhouse_functions WHERE name = ?".to_string(),
                vec![SqlValue::Text(name.to_string())],
            )
            .await?;
        info!("🗑 Deleted function '{}'", name);
        Ok(())
    }

    async fn gen_types(
        store: std::sync::Arc<StackhouseStore>,
        args: &TypesArgs,
    ) -> StackhouseResult<()> {
        let generator = TypeScriptGenerator::new(store);
        let ts = generator.generate().await?;

        if args.stdout {
            println!("{}", ts);
        } else {
            tokio::fs::write(&args.output, ts)
                .await
                .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Write types: {}", e)))?;
            info!("✅ Generated {}", args.output.display());
        }
        Ok(())
    }

    async fn status(store: std::sync::Arc<StackhouseStore>) -> StackhouseResult<()> {
        let rows = store
            .query(
                "SELECT count(*) as total_connections FROM pg_stat_activity".to_string(),
                vec![],
            )
            .await?;

        let total = rows
            .first()
            .and_then(|r| r.first())
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        info!("✅ Stackhouse is up. Active PG connections: {}", total);
        Ok(())
    }

    async fn backup_create(store: std::sync::Arc<StackhouseStore>) -> StackhouseResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        store.execute(
            "INSERT INTO stackhouse_restore_points (id, tenant_id, point_in_time, backup_type, status) VALUES (?, 0, NOW(), 'full', 'available')".to_string(),
            vec![SqlValue::Text(id.clone())],
        ).await?;
        info!("✅ Backup marker created: {}", id);
        Ok(())
    }

    async fn backup_list(store: std::sync::Arc<StackhouseStore>) -> StackhouseResult<()> {
        let rows = store.query(
            "SELECT id, point_in_time, backup_type, status FROM stackhouse_restore_points WHERE tenant_id = 0 ORDER BY point_in_time DESC".to_string(),
            vec![],
        ).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        Ok(())
    }

    async fn branch_list(store: std::sync::Arc<StackhouseStore>) -> StackhouseResult<()> {
        let rows = store.query(
            "SELECT id, name, parent_branch, schema_name, status, created_at FROM stackhouse_db_branches WHERE tenant_id = 0 ORDER BY created_at DESC".to_string(),
            vec![],
        ).await?;
        for row in rows {
            let map: std::collections::HashMap<_, _> = row.into_iter().collect();
            println!("{:?}", map);
        }
        Ok(())
    }

    async fn branch_create(
        store: std::sync::Arc<StackhouseStore>,
        name: &str,
        parent: Option<&str>,
    ) -> StackhouseResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let schema_name = format!("stackhouse_branch_{}", id[..8].to_string());

        store
            .execute(
                format!("CREATE SCHEMA IF NOT EXISTS {}", schema_name),
                vec![],
            )
            .await?;

        let parent_schema = parent.unwrap_or("public");
        let tables = store.query(
            format!("SELECT table_name FROM information_schema.tables WHERE table_schema = '{}' AND table_type = 'BASE TABLE'", parent_schema),
            vec![],
        ).await?;

        for row in tables {
            let table = row
                .iter()
                .find(|(k, _)| k == "table_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            if table.starts_with("stackhouse_") {
                store
                    .execute(
                        format!(
                            "CREATE TABLE IF NOT EXISTS {}.{} (LIKE {}.{} INCLUDING ALL)",
                            schema_name, table, parent_schema, table
                        ),
                        vec![],
                    )
                    .await
                    .ok();
                store
                    .execute(
                        format!(
                            "INSERT INTO {}.{} SELECT * FROM {}.{}",
                            schema_name, table, parent_schema, table
                        ),
                        vec![],
                    )
                    .await
                    .ok();
            }
        }

        store.execute(
            "INSERT INTO stackhouse_db_branches (id, tenant_id, name, parent_branch, schema_name, status) VALUES (?, 0, ?, ?, ?, 'ready')".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(parent.unwrap_or("").to_string()),
                SqlValue::Text(schema_name.clone()),
            ],
        ).await?;

        info!("✅ Branch created: {} (schema: {})", name, schema_name);
        Ok(())
    }

    async fn branch_reset(
        store: std::sync::Arc<StackhouseStore>,
        name: &str,
    ) -> StackhouseResult<()> {
        let rows = store.query(
            "SELECT id, schema_name, parent_branch FROM stackhouse_db_branches WHERE name = ? AND tenant_id = 0".to_string(),
            vec![SqlValue::Text(name.to_string())],
        ).await?;

        let row = rows
            .first()
            .ok_or_else(|| StackhouseError::NotFound("Branch not found".into()))?;
        let _id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let schema = row
            .iter()
            .find(|(k, _)| k == "schema_name")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let parent = row
            .iter()
            .find(|(k, _)| k == "parent_branch")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("public");

        store
            .execute(format!("DROP SCHEMA IF EXISTS {} CASCADE", schema), vec![])
            .await
            .ok();
        store
            .execute(format!("CREATE SCHEMA IF NOT EXISTS {}", schema), vec![])
            .await?;

        let tables = store.query(
            format!("SELECT table_name FROM information_schema.tables WHERE table_schema = '{}' AND table_type = 'BASE TABLE'", parent),
            vec![],
        ).await?;

        for row in tables {
            let table = row
                .iter()
                .find(|(k, _)| k == "table_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            if table.starts_with("stackhouse_") {
                store
                    .execute(
                        format!(
                            "CREATE TABLE IF NOT EXISTS {}.{} (LIKE {}.{} INCLUDING ALL)",
                            schema, table, parent, table
                        ),
                        vec![],
                    )
                    .await
                    .ok();
                store
                    .execute(
                        format!(
                            "INSERT INTO {}.{} SELECT * FROM {}.{}",
                            schema, table, parent, table
                        ),
                        vec![],
                    )
                    .await
                    .ok();
            }
        }

        info!("✅ Branch reset: {}", name);
        Ok(())
    }

    async fn branch_delete(
        store: std::sync::Arc<StackhouseStore>,
        name: &str,
    ) -> StackhouseResult<()> {
        let rows = store.query(
            "SELECT id, schema_name FROM stackhouse_db_branches WHERE name = ? AND tenant_id = 0".to_string(),
            vec![SqlValue::Text(name.to_string())],
        ).await?;

        if let Some(row) = rows.first() {
            let id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let schema = row
                .iter()
                .find(|(k, _)| k == "schema_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            store
                .execute(format!("DROP SCHEMA IF EXISTS {} CASCADE", schema), vec![])
                .await
                .ok();
            store
                .execute(
                    "DELETE FROM stackhouse_db_branches WHERE id = ? AND tenant_id = 0".to_string(),
                    vec![SqlValue::Text(id.to_string())],
                )
                .await?;
        }

        info!("🗑 Branch deleted: {}", name);
        Ok(())
    }

    async fn fixtures_load(
        store: std::sync::Arc<StackhouseStore>,
        file: &PathBuf,
    ) -> StackhouseResult<()> {
        let loader = FixtureLoader::new(store);
        let results = loader.load_from_file(file).await?;
        for r in results {
            info!(
                "Fixture '{}': table={}, inserted={}",
                r.fixture, r.table, r.inserted
            );
        }
        Ok(())
    }

    async fn fixtures_seed(file: &PathBuf) -> StackhouseResult<()> {
        let fixture = fixtures::FixtureLoader::standard_test_fixture();
        let json = serde_json::to_string_pretty(&fixture)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Serialize fixture: {}", e)))?;
        tokio::fs::write(file, json)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Write fixture: {}", e)))?;
        info!("✅ Wrote standard fixture to {}", file.display());
        Ok(())
    }

    async fn init_project(args: &InitArgs) -> StackhouseResult<()> {
        let dir = args
            .dir
            .as_ref()
            .map(|p| p.clone())
            .unwrap_or_else(|| PathBuf::from(&args.name));
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Create project dir: {}", e)))?;

        let config = format!(
            r#"[project]
name = "{}"
url = "postgres://postgres:postgres@localhost:5432/stackhouse"

[dev]
port = 3000
host = "0.0.0.0"
"#,
            args.name
        );
        tokio::fs::write(dir.join("stackhouse.toml"), config)
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Write stackhouse.toml: {}", e))
            })?;

        tokio::fs::create_dir_all(dir.join("functions")).await.ok();

        if args.ts {
            tokio::fs::create_dir_all(dir.join("src")).await.ok();
            tokio::fs::write(dir.join("src").join("stackhouse.ts"), TS_CLIENT_STUB)
                .await
                .map_err(|e| {
                    StackhouseError::Internal(anyhow::anyhow!("Write TS client: {}", e))
                })?;
        }

        if args.python {
            tokio::fs::write(dir.join("stackhouse_client.py"), PY_CLIENT_STUB)
                .await
                .map_err(|e| {
                    StackhouseError::Internal(anyhow::anyhow!("Write Python client: {}", e))
                })?;
        }

        info!(
            "✅ Initialized project '{}' in {}",
            args.name,
            dir.display()
        );
        Ok(())
    }
}

const TS_CLIENT_STUB: &str = r#"import { StackhouseResponse, QueryOptions, AuthTokens } from './stackhouse.types';

export class StackhouseClient {
  constructor(private baseUrl: string, private apiKey?: string) {}

  async query<T = any>(table: string, options?: QueryOptions): Promise<StackhouseResponse<T[]>> {
    const url = new URL(`${this.baseUrl}/v1/query/${table}`);
    if (options) {
      for (const [k, v] of Object.entries(options)) {
        if (v !== undefined) url.searchParams.set(k, String(v));
      }
    }
    return this.request<StackhouseResponse<T[]>>(url.toString());
  }

  async push<T = any>(table: string, data: T): Promise<StackhouseResponse<T>> {
    return this.request<StackhouseResponse<T>>(`${this.baseUrl}/v1/push/${table}`, 'POST', data);
  }

  private async request<T>(url: string, method: string = 'GET', body?: any): Promise<T> {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (this.apiKey) headers['Authorization'] = `Bearer ${this.apiKey}`;
    const res = await fetch(url, { method, headers, body: body ? JSON.stringify(body) : undefined });
    return res.json() as Promise<T>;
  }
}
"#;

const PY_CLIENT_STUB: &str = r#"import requests

class StackhouseClient:
    def __init__(self, base_url: str, api_key: str = None):
        self.base_url = base_url
        self.api_key = api_key

    def _headers(self):
        h = {"Content-Type": "application/json"}
        if self.api_key:
            h["Authorization"] = f"Bearer {self.api_key}"
        return h

    def query(self, table: str, **kwargs):
        return requests.get(f"{self.base_url}/v1/query/{table}", params=kwargs, headers=self._headers()).json()

    def push(self, table: str, data: dict):
        return requests.post(f"{self.base_url}/v1/push/{table}", json=data, headers=self._headers()).json()
"#;
