//! # 🛸 Stackhouse
//!
//! A high-performance, "Schema-Later" database that dynamically evolves
//! its schema based on incoming JSON payloads.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::http::HeaderValue;
use axum::middleware::from_fn_with_state;
use clap::Parser;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use stackhouse::api::{
    build_schema, create_admin_router, create_graphql_router, create_mcp_router,
    create_platform_router, create_router, AdminAuditService, AdminState, AppState, GraphQLState,
    McpServer, McpState, PlatformState,
};
use stackhouse::auth::{
    create_auth_router, create_captcha_router, create_magic_link_router, create_mfa_router,
    create_oauth_router, create_phone_otp_router, token_blacklist_middleware, ApiKeyService,
    AuthService, AuthState, CaptchaConfig, CaptchaService, CaptchaState, EmailConfig,
    MagicLinkService, MagicLinkState, MfaService, MfaState, OAuthProvider, OAuthService,
    OAuthState, PhoneOtpService, PhoneOtpState, ProviderConfig, TwilioConfig,
};
use stackhouse::branching::{create_branching_router, BranchingService, BranchingState};
use stackhouse::cli::{Cli, CliRunner, Command};
use stackhouse::extensions::{create_extensions_router, ExtensionsService, ExtensionsState};
use stackhouse::image_transform::{
    create_image_transform_router, ImageTransformService, ImageTransformState,
};
use stackhouse::platform::db::StackhouseStore;
use stackhouse::platform::{
    create_log_drain_router, create_metrics_router, create_replicas_router, DrainType,
    LogDrainConfig, LogDrainService, LogDrainState, LogLevel, MetricsState, ProvisioningService,
    ReplicaService, ReplicaState, StackhouseMetrics,
};
use stackhouse::realtime::{
    create_broadcast_router, create_presence_router, create_realtime_router, BroadcastService,
    BroadcastState, PresenceService, PresenceState, RealtimeEngine, RealtimeState,
};
use stackhouse::security::{
    create_network_router, create_rls_router, network_middleware, AuthorizationService,
    NetworkService, NetworkState, RlsService, RlsState, SchemaGuard, SecurityConfig,
};
use stackhouse::storage::{
    create_backup_router, create_explorer_router, create_storage_router, create_vector_router,
    BackupService, BackupState, PitrService, StorageService, StorageState, VectorService,
    VectorState,
};
use stackhouse::teams::{create_teams_router, TeamsService, TeamsState};

// === Edge Functions & Compute ===
use stackhouse::compute::{
    create_functions_router, create_secrets_router, create_webhooks_router, EventBus,
    FunctionsService, FunctionsState, JobQueue, SecretsState, SecretsVault, WebhookService,
    WebhookState,
};

// === Platform Extensions ===
use stackhouse::platform::multi_tenancy::MultiTenancyService;
use stackhouse::platform::observability::ObservabilityService;

fn print_banner(port: u16, in_memory: bool, db_url: &str) {
    println!(
        r#"
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║   🧠  ██╗   ██╗██╗██████╗ ███████╗██████╗ ██████╗               ║
║       ██║   ██║██║██╔══██╗██╔════╝██╔══██╗██╔══██╗              ║
║       ██║   ██║██║██████╔╝█████╗  ██║  ██║██████╔╝              ║
║       ╚██╗ ██╔╝██║██╔══██╗██╔══╝  ██║  ██║██╔══██╗              ║
║        ╚████╔╝ ██║██████╔╝███████╗██████╔╝██████╔╝              ║
║         ╚═══╝  ╚═╝╚═════╝ ╚══════╝╚═════╝ ╚═════╝               ║
║                                                                  ║
║   Schema-Later Postgres + Vector Search                          ║
║                                                                  ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║   🌐 API:       http://localhost:{:<5}                          ║
║   🧠 Vectors:   http://localhost:{:<5}/v1/vectors               ║
║   📊 Explorer:  http://localhost:{:<5}/explore                  ║
║   📈 GraphQL:   http://localhost:{:<5}/v1/graphql              ║
║   ⚡ Realtime:  ws://localhost:{:<5}/v1/realtime               ║
║   📊 Metrics:   http://localhost:{:<5}/metrics                 ║
║   💾 Database:  {:<45} ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
"#,
        port,
        port,
        port,
        port,
        port,
        port,
        if in_memory { "test db" } else { db_url }
    );
}

fn env_flag(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("on")
    )
}

fn cors_allowed_origins() -> Vec<HeaderValue> {
    let raw = env::var("STACKHOUSE_CORS_ALLOWED_ORIGINS").unwrap_or_default();

    if raw.trim().is_empty() {
        // Default to the ports the bundled UI actually runs on (Vite dev
        // server + `vite preview`), so `npm run dev` works against a
        // freshly cloned backend without extra CORS configuration.
        return vec![
            HeaderValue::from_static("http://localhost:5173"),
            HeaderValue::from_static("http://localhost:4173"),
            HeaderValue::from_static("http://localhost:8080"),
        ];
    }

    raw.split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .filter_map(|origin| {
            if origin == "*" {
                tracing::warn!("Ignoring wildcard CORS origin; configure explicit origins instead");
                return None;
            }

            match HeaderValue::from_str(origin) {
                Ok(value) => Some(value),
                Err(err) => {
                    tracing::warn!("Ignoring invalid CORS origin '{}': {}", origin, err);
                    None
                }
            }
        })
        .collect()
}

fn cors_layer() -> CorsLayer {
    let origins = cors_allowed_origins();
    if origins.is_empty() {
        info!("CORS allowlist is empty; cross-origin requests will be rejected");
    } else {
        info!("CORS allowlist configured for {} origin(s)", origins.len());
    }

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(Any)
        .allow_headers(Any)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    let cli = Cli::parse();

    // Determine DB URL from CLI or env
    let db_url = cli
        .url
        .clone()
        .or_else(|| env::var("STACKHOUSE_URL").ok())
        .unwrap_or_else(|| "postgres://postgres:postgres@localhost:5432/stackhouse".to_string());

    match &cli.command {
        Command::Serve(args) => run_server(&cli, args, &db_url).await,
        other => {
            // Initialize a DB store for CLI commands (skip server setup)
            let store = Arc::new(StackhouseStore::new(&db_url).await?);
            CliRunner::run(&cli, other, store).await?;
            Ok(())
        }
    }
}

async fn run_server(cli: &Cli, args: &stackhouse::cli::ServeArgs, db_url: &str) -> Result<()> {
    let base_url = cli
        .base_url
        .clone()
        .or_else(|| env::var("STACKHOUSE_BASE_URL").ok())
        .unwrap_or_else(|| format!("http://localhost:{}", args.port));

    // Initialize database
    let store = if args.memory {
        info!("🧪 Using test database");
        Arc::new(StackhouseStore::in_memory().await?)
    } else {
        info!("💾 Using database: {}", db_url);
        Arc::new(StackhouseStore::new(db_url).await?)
    };

    // Initialize JWT secret
    let jwt_secret = args
        .jwt_secret
        .clone()
        .or_else(|| env::var("STACKHOUSE_JWT_SECRET").ok())
        .map(|s| s.into_bytes())
        .unwrap_or_else(|| {
            panic!("STACKHOUSE_JWT_SECRET environment variable is required. Set it to a secure random string.");
        });

    // ====================================================================
    // Initialize Core Services
    // ====================================================================

    let auth_service = AuthService::new(Arc::clone(&store), jwt_secret.clone()).await?;
    let auth_state = AuthState {
        auth: auth_service.clone(),
    };
    let api_key_service = Arc::new(ApiKeyService::new(Arc::clone(&store)).await?);
    let authorization_service = AuthorizationService::new(SecurityConfig::default());

    let storage_path = args
        .storage_path
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./stackhouse_storage"));
    let storage_service =
        StorageService::new(Arc::clone(&store), Some(storage_path.clone())).await?;
    let storage_state = StorageState {
        storage: storage_service,
    };

    let guard = Arc::new(SchemaGuard::new(Arc::clone(&store)));
    let qdrant_url_vec =
        env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".to_string());
    let vector_service = VectorService::new(qdrant_url_vec).await?;
    let vector_state = VectorState {
        vector: vector_service,
    };

    let rls_service = Arc::new(RlsService::new(Arc::clone(&store)).await?);
    let rls_state = RlsState {
        rls: Arc::clone(&rls_service),
    };

    let realtime_engine = RealtimeEngine::new();
    let realtime_state = RealtimeState {
        realtime: realtime_engine,
    };

    // ====================================================================
    // Initialize Auth Enhancement Services
    // ====================================================================

    // OAuth2
    let mut oauth_service = OAuthService::new(
        Arc::clone(&store),
        auth_service.clone(),
        base_url.clone(),
        jwt_secret.clone(),
    )
    .await?;
    if let (Ok(id), Ok(secret)) = (
        env::var("STACKHOUSE_GOOGLE_CLIENT_ID"),
        env::var("STACKHOUSE_GOOGLE_CLIENT_SECRET"),
    ) {
        oauth_service.register_provider(OAuthProvider::Google, ProviderConfig::google(id, secret));
    }
    if let (Ok(id), Ok(secret)) = (
        env::var("STACKHOUSE_GITHUB_CLIENT_ID"),
        env::var("STACKHOUSE_GITHUB_CLIENT_SECRET"),
    ) {
        oauth_service.register_provider(OAuthProvider::Github, ProviderConfig::github(id, secret));
    }
    if let (Ok(id), Ok(secret)) = (
        env::var("STACKHOUSE_DISCORD_CLIENT_ID"),
        env::var("STACKHOUSE_DISCORD_CLIENT_SECRET"),
    ) {
        oauth_service
            .register_provider(OAuthProvider::Discord, ProviderConfig::discord(id, secret));
    }
    if let (Ok(id), Ok(secret)) = (
        env::var("STACKHOUSE_APPLE_CLIENT_ID"),
        env::var("STACKHOUSE_APPLE_CLIENT_SECRET"),
    ) {
        oauth_service.register_provider(OAuthProvider::Apple, ProviderConfig::apple(id, secret));
    }
    let oauth_state = OAuthState {
        oauth: oauth_service,
    };

    // Magic Link
    let email_config = EmailConfig::from_env();
    let magic_link_service = MagicLinkService::new(
        Arc::clone(&store),
        auth_service.clone(),
        email_config,
        base_url.clone(),
        jwt_secret.clone(),
    )
    .await?;
    let magic_link_state = MagicLinkState {
        magic_link: magic_link_service,
    };

    // MFA / TOTP
    let mfa_service = MfaService::new(
        Arc::clone(&store),
        auth_service.clone(),
        "Stackhouse".to_string(),
        false,
    )
    .await?;
    let mfa_state = MfaState { mfa: mfa_service };

    // Phone OTP (Twilio)
    let twilio_config = TwilioConfig::from_env();
    let phone_otp_service =
        PhoneOtpService::new(Arc::clone(&store), auth_service.clone(), twilio_config).await?;
    let phone_otp_state = PhoneOtpState {
        phone_otp: phone_otp_service,
    };

    // Captcha
    let captcha_config = CaptchaConfig::from_env();
    let captcha_service = Arc::new(CaptchaService::new(captcha_config));
    let captcha_state = CaptchaState {
        captcha: captcha_service,
    };

    // ====================================================================
    // Initialize API & Observability Services
    // ====================================================================

    // GraphQL
    let graphql_schema = build_schema(Arc::clone(&store), Arc::clone(&guard));
    let graphql_state = GraphQLState {
        schema: graphql_schema,
    };

    // Prometheus Metrics
    let metrics = Arc::new(StackhouseMetrics::new());
    let metrics_state = MetricsState {
        metrics: Arc::clone(&metrics),
    };

    // Log Drains
    let mut log_drains = vec![LogDrainConfig {
        name: "database".to_string(),
        drain_type: DrainType::Database,
        url: None,
        api_key: None,
        min_level: LogLevel::Info,
        enabled: true,
    }];
    if let Ok(url) = env::var("STACKHOUSE_LOG_DRAIN_URL") {
        log_drains.push(LogDrainConfig {
            name: "webhook".to_string(),
            drain_type: DrainType::Webhook,
            url: Some(url),
            api_key: env::var("STACKHOUSE_LOG_DRAIN_KEY").ok(),
            min_level: LogLevel::Warn,
            enabled: true,
        });
    }
    let log_drain_service = LogDrainService::new(Arc::clone(&store), log_drains).await?;
    let admin_audit = Arc::new(AdminAuditService::new(Arc::clone(&store)).await?);
    let admin_state = AdminState {
        audit: (*admin_audit).clone(),
    };

    // Management / Provisioning API
    let partner_keys = env::var("STACKHOUSE_PARTNER_KEYS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let provisioning_service = ProvisioningService::new(
        Arc::clone(&store),
        storage_path.clone(),
        partner_keys,
        base_url.clone(),
    )
    .await?;
    let platform_state = PlatformState {
        provisioning: provisioning_service,
        audit: Arc::clone(&admin_audit),
        db_url: db_url.to_string(),
        base_url: base_url.clone(),
    };

    let log_drain_state = LogDrainState {
        log_drain: log_drain_service,
        auth: auth_state.clone(),
        authorization: authorization_service.clone(),
        admin_audit: Arc::clone(&admin_audit),
    };

    // ====================================================================
    // Initialize Platform Services
    // ====================================================================

    // Image Transforms
    let transform_service = Arc::new(ImageTransformService::new(storage_path.clone()));
    let transform_state = ImageTransformState {
        transform: transform_service,
    };

    // Presence
    let presence_service = Arc::new(PresenceService::new());
    let presence_state = PresenceState {
        presence: presence_service,
    };

    // Broadcast
    let broadcast_service = Arc::new(BroadcastService::new());
    let broadcast_state = BroadcastState {
        broadcast: broadcast_service,
    };
    // Extensions
    let extensions_service = Arc::new(ExtensionsService::new(Arc::clone(&store)));
    let extensions_state = ExtensionsState {
        extensions: extensions_service,
        auth: auth_state.clone(),
        authorization: authorization_service.clone(),
        admin_audit: Arc::clone(&admin_audit),
    };

    // Branching
    let branching_service = Arc::new(BranchingService::new(Arc::clone(&store)));
    let branching_state = BranchingState {
        branching: branching_service,
        auth: auth_state.clone(),
        authorization: authorization_service.clone(),
        admin_audit: Arc::clone(&admin_audit),
    };

    // Teams
    let teams_service = Arc::new(TeamsService::new(Arc::clone(&store)).await?);
    let teams_state = TeamsState {
        teams: teams_service,
        auth: auth_state.clone(),
    };

    // Network
    let network_service = Arc::new(NetworkService::new());
    let network_state = NetworkState {
        network: network_service,
        auth: auth_state.clone(),
        authorization: authorization_service.clone(),
        admin_audit: Arc::clone(&admin_audit),
    };

    // Backup & PITR
    let backup_path = storage_path.join("backups");
    let backup_service =
        Arc::new(BackupService::new(Arc::clone(&store), backup_path, db_url.to_string()).await?);
    let pitr_service = Arc::new(PitrService::new(Arc::clone(&store)).await?);
    let backup_state = BackupState {
        backup: backup_service,
        pitr: pitr_service,
        auth: auth_state.clone(),
        authorization: authorization_service.clone(),
        admin_audit: Arc::clone(&admin_audit),
    };

    // Read replicas
    let replica_service = Arc::new(ReplicaService::new(Arc::clone(&store)).await?);
    let replica_state = ReplicaState {
        replicas: replica_service,
        auth: auth_state.clone(),
    };

    // ====================================================================
    // Initialize Billing (RevenueCat-style, opt-in)
    // ====================================================================

    let billing_enabled = env_flag("STACKHOUSE_ENABLE_BILLING");
    let billing_state = if billing_enabled {
        let billing_store = stackhouse::billing::init(Arc::clone(&store)).await?;
        let billing_http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("billing http client");
        let billing_config = stackhouse::billing::BillingConfig {
            default_apple_shared_secret: env::var("STACKHOUSE_BILLING_APPLE_SHARED_SECRET").ok(),
            default_stripe_signing_secret: env::var("STACKHOUSE_BILLING_STRIPE_SIGNING_SECRET")
                .ok(),
            default_google_access_token: env::var("STACKHOUSE_BILLING_GOOGLE_ACCESS_TOKEN").ok(),
        };
        // Spawn outbound webhook dispatcher.
        stackhouse::billing::webhooks::spawn_dispatcher(
            Arc::clone(&billing_store),
            billing_http.clone(),
        );
        info!("💳 Stackhouse-Billing enabled (mounted at /v1/billing)");
        Some(stackhouse::billing::BillingState::new(
            billing_store,
            billing_http,
            billing_config,
            auth_state.clone(),
            authorization_service.clone(),
        ))
    } else {
        None
    };

    // ====================================================================
    // Initialize Edge Functions & Compute
    // ====================================================================

    let function_runtime = Arc::new(FunctionsService::new(Arc::clone(&store)).await?);
    let webhook_service = Arc::new(WebhookService::new(Arc::clone(&store)).await?);
    let _event_bus = Arc::new(EventBus::new(Arc::clone(&store)).await?);
    let _job_queue = Arc::new(JobQueue::new(Arc::clone(&store)).await?);
    let secrets_vault = Arc::new(SecretsVault::new(Arc::clone(&store)).await?);

    let webhook_state = WebhookState {
        webhooks: Arc::clone(&webhook_service),
        auth: auth_state.clone(),
    };
    let secrets_state = SecretsState {
        vault: Arc::clone(&secrets_vault),
        auth: auth_state.clone(),
    };
    let functions_state = FunctionsState {
        functions: Arc::clone(&function_runtime),
        auth: auth_state.clone(),
    };

    info!("⚡ Edge Functions & Compute services initialized");

    // ====================================================================
    // Initialize Multi-Tenancy & Observability
    // ====================================================================

    let _multi_tenancy = Arc::new(MultiTenancyService::new(Arc::clone(&store)).await?);
    let _observability = Arc::new(ObservabilityService::new(Arc::clone(&store)).await?);

    info!("🏢 Multi-tenancy & observability services initialized");

    // MCP Server (read + scoped write tools)
    let mcp_server = McpServer::new(Arc::clone(&store), Arc::clone(&api_key_service));
    let mcp_state = McpState { mcp: mcp_server };

    // ====================================================================
    // Build Router
    // ====================================================================

    let state = AppState::with_security(
        Arc::clone(&store),
        auth_state.clone(),
        authorization_service.clone(),
        env_flag("STACKHOUSE_ENABLE_RAW_SQL"),
        env_flag("STACKHOUSE_ENABLE_DESTRUCTIVE_ADMIN"),
        Some(admin_audit),
    )
    .with_rls(Arc::clone(&rls_service));

    let network_middleware_state = Arc::clone(&network_state.network);

    let app = create_router(state)
        // === Auth ===
        .nest("/v1/auth", create_auth_router(auth_state.clone()))
        .nest("/v1/auth", create_oauth_router(oauth_state))
        .nest("/v1/auth", create_magic_link_router(magic_link_state))
        .nest("/v1/auth", create_mfa_router(mfa_state))
        .nest("/v1/auth", create_phone_otp_router(phone_otp_state))
        .nest("/v1/auth", create_captcha_router(captcha_state))
        // === Data / Storage ===
        .nest("/v1/storage", create_storage_router(storage_state))
        .nest(
            "/v1/storage",
            create_image_transform_router(transform_state),
        )
        .nest("/v1/vectors", create_vector_router(vector_state))
        .nest("/v1/rls", create_rls_router(rls_state))
        // === Realtime ===
        .nest("/v1/realtime", create_realtime_router(realtime_state))
        .nest("/v1/realtime", create_presence_router(presence_state))
        .nest("/v1/realtime", create_broadcast_router(broadcast_state))
        // === API ===
        .nest("/v1", create_graphql_router(graphql_state))
        // === Admin / Platform ===
        .nest("/v1/admin", create_extensions_router(extensions_state))
        .nest("/v1/admin", create_branching_router(branching_state))
        .nest("/v1/admin", create_network_router(network_state))
        .nest("/v1/admin", create_backup_router(backup_state))
        .nest("/v1/admin", create_log_drain_router(log_drain_state))
        .nest("/v1/admin", create_admin_router(admin_state))
        .nest("/v1/platform", create_platform_router(platform_state))
        .nest(
            "/v1/platform/replicas",
            create_replicas_router(replica_state),
        )
        // === MCP ===
        .nest("/", create_mcp_router(mcp_state))
        // === Teams ===
        .nest("/v1", create_teams_router(teams_state))
        // === Compute ===
        .nest("/v1/functions", create_functions_router(functions_state))
        .nest("/v1/webhooks", create_webhooks_router(webhook_state))
        .nest("/v1/secrets", create_secrets_router(secrets_state));

    // === Billing (opt-in) ===
    let app = if let Some(bs) = billing_state {
        app.nest(
            "/v1/billing",
            stackhouse::billing::create_billing_router(bs),
        )
    } else {
        app
    };

    let app = app
        // === Observability ===
        .merge(create_metrics_router(metrics_state))
        // === Explorer ===
        .merge(create_explorer_router())
        .layer(from_fn_with_state(
            network_middleware_state,
            network_middleware,
        ))
        // Token blacklist check for authenticated routes
        .layer(from_fn_with_state(
            auth_state.clone(),
            token_blacklist_middleware,
        ))
        // Middleware applied to ALL routes
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http());

    // Print banner
    print_banner(args.port, args.memory, db_url);

    // Start server
    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .expect("Invalid address");

    info!("🚀 Stackhouse listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
    })
    .await?;

    Ok(())
}
