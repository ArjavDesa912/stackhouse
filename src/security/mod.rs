pub mod authorization {
    pub use crate::authorization::*;
}

pub mod guard {
    pub use crate::guard::*;
}

pub mod network {
    pub use crate::network::*;
}

pub mod rls {
    pub use crate::rls::*;
}

pub mod abac;
pub mod bot_protection;
pub mod byok;
pub mod data_residency;
pub mod egress;
pub mod encryption;
pub mod gdpr;
pub mod headers;
pub mod private_network;
pub mod rls_policies;
pub mod vuln_scan;
pub mod waf;

pub use abac::*;
pub use authorization::*;
pub use bot_protection::*;
pub use byok::*;
pub use data_residency::*;
pub use egress::*;
pub use encryption::*;
pub use gdpr::*;
pub use guard::*;
pub use headers::*;
pub use network::*;
pub use private_network::*;
pub use rls::*;
pub use rls_policies::*;
pub use vuln_scan::*;
pub use waf::{waf_middleware, WafConfig, WafService, WafStats};

/// Constant-time comparison for secrets (tokens, API keys, signatures).
/// Returns true if `a` and `b` have identical length and content.
pub fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}
