//! Shared state for the billing module.

use reqwest::Client;
use std::sync::Arc;

use crate::auth::AuthState;
use crate::authorization::AuthorizationService;

use super::store::BillingStore;

#[derive(Clone, Debug, Default)]
pub struct BillingConfig {
    /// Fallback Apple shared secret when none is configured per-app.
    pub default_apple_shared_secret: Option<String>,
    /// Fallback Stripe signing secret when none is configured per-app.
    pub default_stripe_signing_secret: Option<String>,
    /// Fallback Google OAuth access token (service-account) when none is configured per-app.
    pub default_google_access_token: Option<String>,
}

#[derive(Clone)]
pub struct BillingState {
    pub store: Arc<BillingStore>,
    pub http: Client,
    pub config: Arc<BillingConfig>,
    pub auth: AuthState,
    pub authorization: AuthorizationService,
}

impl BillingState {
    pub fn new(
        store: Arc<BillingStore>,
        http: Client,
        config: BillingConfig,
        auth: AuthState,
        authorization: AuthorizationService,
    ) -> Self {
        Self {
            store,
            http,
            config: Arc::new(config),
            auth,
            authorization,
        }
    }
}
