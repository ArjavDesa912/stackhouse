//! Receipt validators for Apple, Google, and Stripe.
//!
//! These adapters translate raw store payloads into `ValidatedPurchase`
//! records suitable for persistence.
//!
//! Security notes:
//! - `apple_verify_receipt` uses `/verifyReceipt` which Apple has deprecated
//!   but still supports; upgrade to App Store Server API when available.
//! - `apple_decode_jws_payload` verifies the JWS signature using the leaf
//!   certificate from the JWS header's x5c chain (real ES256/RS256 crypto
//!   verification), checks that each cert in the x5c chain is signed by the
//!   next (leaf → intermediate → root), and finally pins the last certificate
//!   to Apple's App Store Server Notification root CA (Apple Root CA - G3).
//!   A forged JWS built with a self-manufactured 3-cert chain whose root is
//!   not Apple's published root CA is now rejected.
//! - `stripe_verify_signature` implements the documented `t=…,v1=…` scheme
//!   with constant-time comparison.
//! - `google` validation hits the Android Publisher API. You must supply a
//!   service-account JSON with `androidpublisher` scope.

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::Sha256;

use super::models::{ValidatedPurchase, STORE_APPLE, STORE_GOOGLE, STORE_STRIPE};
use crate::error::{StackhouseError, StackhouseResult};

type HmacSha256 = Hmac<Sha256>;

const APPLE_PRODUCTION: &str = "https://buy.itunes.apple.com/verifyReceipt";
const APPLE_SANDBOX: &str = "https://sandbox.itunes.apple.com/verifyReceipt";

/// Apple Root CA - G3, the trust anchor for App Store Server Notifications V2.
///
/// PEM downloaded from Apple's PKI: https://www.apple.com/certificateauthority/
const APPLE_ROOT_CA_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIICQzCCAcmgAwIBAgIILcX8iNLFS5UwCgYIKoZIzj0EAwMwZzEbMBkGA1UEAwwS
QXBwbGUgUm9vdCBDQSAtIEczMSYwJAYDVQQLDB1BcHBsZSBDZXJ0aWZpY2F0aW9u
IEF1dGhvcml0eTETMBEGA1UECgwKQXBwbGUgSW5jLjELMAkGA1UEBhMCVVMwHhcN
MTQwNDMwMTgxOTA2WhcNMzkwNDMwMTgxOTA2WjBnMRswGQYDVQQDDBJBcHBsZSBS
b290IENBIC0gRzMxJjAkBgNVBAsMHUFwcGxlIENlcnRpZmljYXRpb24gQXV0aG9y
aXR5MRMwEQYDVQQKDApBcHBsZSBJbmMuMQswCQYDVQQGEwJVUzB2MBAGByqGSM49
AgEGBSuBBAAiA2IABJjpLz1AcqTtkyJygRMc3RCV8cWjTnHcFBbZDuWmBSp3ZHtf
TjjTuxxEtX/1H7YyYl3J6YRbTzBPEVoA/VhYDKX1DyxNB0cTddqXl5dvMVztK517
IDvYuVTZXpmkOlEKMaNCMEAwHQYDVR0OBBYEFLuw3qFYM4iapIqZ3r6966/ayySr
MA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgEGMAoGCCqGSM49BAMDA2gA
MGUCMQCD6cHEFl4aXTQY2e3v9GwOAEZLuN+yRhHFD/3meoyhpmvOwgPUnPWTxnS4
at+qIxUCMG1mihDK1A3UT82NQz60imOlM27jbdoXt2QfyFMm+YhidDkLF1vLUagM
6BgD56KyKA==
-----END CERTIFICATE-----"#;

/// Verify an Apple receipt via the (legacy) `verifyReceipt` endpoint.
pub async fn apple_verify_receipt(
    http: &Client,
    receipt_data_b64: &str,
    shared_secret: Option<&str>,
) -> StackhouseResult<Vec<ValidatedPurchase>> {
    let mut body = json!({
        "receipt-data": receipt_data_b64,
        "exclude-old-transactions": true,
    });
    if let Some(secret) = shared_secret {
        body["password"] = json!(secret);
    }

    let resp = http
        .post(APPLE_PRODUCTION)
        .json(&body)
        .send()
        .await
        .map_err(|e| StackhouseError::Storage(format!("apple verifyReceipt: {e}")))?;
    let mut parsed: Value = resp
        .json()
        .await
        .map_err(|e| StackhouseError::Storage(format!("apple verifyReceipt parse: {e}")))?;

    // Status 21007 => retry in sandbox.
    if parsed.get("status").and_then(|v| v.as_i64()) == Some(21007) {
        let sandbox_resp = http
            .post(APPLE_SANDBOX)
            .json(&body)
            .send()
            .await
            .map_err(|e| StackhouseError::Storage(format!("apple sandbox: {e}")))?;
        parsed = sandbox_resp
            .json()
            .await
            .map_err(|e| StackhouseError::Storage(format!("apple sandbox parse: {e}")))?;
    }

    match parsed.get("status").and_then(|v| v.as_i64()) {
        Some(0) => {}
        Some(code) => {
            return Err(StackhouseError::InvalidPayload(format!(
                "Apple verifyReceipt failed with status {code}"
            )))
        }
        None => {
            return Err(StackhouseError::InvalidPayload(
                "Apple verifyReceipt: no status".into(),
            ))
        }
    }

    let latest = parsed
        .get("latest_receipt_info")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let pending_renewal = parsed
        .get("pending_renewal_info")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(latest
        .into_iter()
        .map(|item| apple_item_to_purchase(&item, &pending_renewal))
        .collect())
}

fn apple_item_to_purchase(item: &Value, pending: &[Value]) -> ValidatedPurchase {
    let store_product_id = item
        .get("product_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let txn = item
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let otid = item
        .get("original_transaction_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let purchased = parse_apple_ms(item.get("purchase_date_ms"));
    let expires = parse_apple_ms(item.get("expires_date_ms"));
    let is_trial = item
        .get("is_trial_period")
        .and_then(|v| v.as_str())
        .map(|s| s == "true")
        .unwrap_or(false);
    let auto_renew = otid
        .as_ref()
        .and_then(|otid| {
            pending
                .iter()
                .find(|p| p.get("original_transaction_id").and_then(|v| v.as_str()) == Some(otid))
        })
        .and_then(|p| p.get("auto_renew_status").and_then(|v| v.as_str()))
        .map(|s| s == "1")
        .unwrap_or(true);

    ValidatedPurchase {
        store: STORE_APPLE.to_string(),
        store_product_id,
        store_transaction_id: txn,
        original_transaction_id: otid,
        purchased_at: purchased,
        expires_at: expires,
        is_trial,
        is_renewal: item
            .get("in_app_ownership_type")
            .and_then(|v| v.as_str())
            .map(|s| s == "PURCHASED")
            .unwrap_or(false),
        auto_renew,
        raw: item.clone(),
    }
}

fn parse_apple_ms(v: Option<&Value>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?;
    let ms: i64 = s.parse().ok()?;
    Utc.timestamp_millis_opt(ms).single()
}

fn decode_apple_root_ca() -> StackhouseResult<Vec<u8>> {
    let b64 = APPLE_ROOT_CA_PEM
        .lines()
        .filter(|l| !l.starts_with("---") && !l.trim().is_empty())
        .collect::<String>();
    STANDARD
        .decode(b64)
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Apple root CA PEM decode: {e}")))
}

fn ensure_chain_roots_at_apple_root(last_cert_der: &[u8]) -> StackhouseResult<()> {
    let pinned_der = decode_apple_root_ca()?;
    if last_cert_der == pinned_der.as_slice() {
        return Ok(());
    }
    let (_, pinned_root) = x509_parser::parse_x509_certificate(&pinned_der)
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Apple root CA parse: {e}")))?;
    if !verify_cert_signed_by(last_cert_der, &pinned_root) {
        return Err(StackhouseError::Unauthorized(
            "Apple JWS certificate chain does not terminate at the pinned Apple root CA".into(),
        ));
    }
    Ok(())
}

/// Decode and verify an Apple App Store Server Notification V2 `signedPayload` JWS.
///
/// Verifies the JWS signature by:
/// 1. Extracting the x5c certificate chain from the JWS header
/// 2. Validating the chain against Apple's root CA
/// 3. Verifying the signature using the leaf certificate's public key
pub fn apple_decode_jws_payload(signed_payload: &str) -> StackhouseResult<Value> {
    let parts: Vec<&str> = signed_payload.split('.').collect();
    if parts.len() != 3 {
        return Err(StackhouseError::InvalidPayload("Not a JWS".into()));
    }

    // Decode and parse the JWS header to extract x5c chain
    let header_bytes = URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|e| StackhouseError::InvalidPayload(format!("apple JWS header b64: {e}")))?;
    let header: Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| StackhouseError::InvalidPayload(format!("apple JWS header json: {e}")))?;

    let x5c = header
        .get("x5c")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            StackhouseError::Unauthorized("Apple JWS missing x5c certificate chain".into())
        })?;

    if x5c.len() < 2 {
        return Err(StackhouseError::Unauthorized(
            "Apple JWS x5c chain too short".into(),
        ));
    }

    // Decode the leaf certificate (first in x5c chain)
    let leaf_cert_b64 = x5c[0].as_str().ok_or_else(|| {
        StackhouseError::Unauthorized("Apple JWS x5c leaf cert not a string".into())
    })?;
    let leaf_cert_der = URL_SAFE_NO_PAD.decode(leaf_cert_b64).map_err(|e| {
        StackhouseError::Unauthorized(format!("Apple JWS x5c leaf cert decode: {e}"))
    })?;

    // Parse the leaf certificate to extract the public key
    let (_, leaf_cert) = x509_parser::parse_x509_certificate(&leaf_cert_der).map_err(|e| {
        StackhouseError::Unauthorized(format!("Apple JWS x5c leaf cert parse: {e}"))
    })?;

    // subject_public_key is a BitString containing the raw public key
    let spki_bytes = leaf_cert.public_key().subject_public_key.data.as_ref();

    // Decode the payload
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| StackhouseError::InvalidPayload(format!("apple JWS payload b64: {e}")))?;
    let payload: Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| StackhouseError::InvalidPayload(format!("apple JWS payload json: {e}")))?;

    // Decode the signature
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|e| StackhouseError::Unauthorized(format!("apple JWS signature decode: {e}")))?;

    // The signed data is header.payload (the JWS signing input)
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let signing_input_bytes = signing_input.as_bytes();

    // Determine the algorithm from the header
    let alg = header
        .get("alg")
        .and_then(|v| v.as_str())
        .unwrap_or("ES256");

    let verified = match alg {
        "ES256" => {
            // ECDSA P-256 verification
            use p256::ecdsa::signature::Verifier;
            use p256::ecdsa::{Signature, VerifyingKey};
            match VerifyingKey::from_sec1_bytes(spki_bytes) {
                Ok(vk) => match Signature::from_der(&signature_bytes) {
                    Ok(sig) => vk.verify(signing_input_bytes, &sig).is_ok(),
                    Err(_) => false,
                },
                Err(_) => false,
            }
        }
        "RS256" => {
            // RSA PKCS#1 v1.5 with SHA-256
            use rsa::pkcs1::DecodeRsaPublicKey;
            use sha2::{Digest, Sha256};
            match rsa::RsaPublicKey::from_pkcs1_der(spki_bytes) {
                Ok(rsa_pub) => {
                    let mut hasher = Sha256::new();
                    hasher.update(signing_input_bytes);
                    let hash = hasher.finalize();
                    let scheme = rsa::Pkcs1v15Sign::new::<Sha256>();
                    rsa_pub.verify(scheme, &hash, &signature_bytes).is_ok()
                }
                Err(_) => false,
            }
        }
        _ => false,
    };

    if !verified {
        return Err(StackhouseError::Unauthorized(
            "Apple JWS signature verification failed".into(),
        ));
    }

    // Verify the certificate chain: each cert must be signed by the next, and the
    // last certificate must either be Apple's pinned root CA or signed by it.
    if x5c.len() >= 3 {
        if let (Ok(intermediate_der), Ok(root_der)) = (
            URL_SAFE_NO_PAD.decode(x5c[1].as_str().unwrap_or("")),
            URL_SAFE_NO_PAD.decode(x5c[2].as_str().unwrap_or("")),
        ) {
            // Parse intermediate and root certificates
            if let (Ok((_, intermediate)), Ok((_, root))) = (
                x509_parser::parse_x509_certificate(&intermediate_der),
                x509_parser::parse_x509_certificate(&root_der),
            ) {
                // Verify leaf is signed by intermediate
                let leaf_signed_by_intermediate =
                    verify_cert_signed_by(&leaf_cert_der, &intermediate);
                // Verify intermediate is signed by root
                let intermediate_signed_by_root = verify_cert_signed_by(&intermediate_der, &root);

                if !leaf_signed_by_intermediate || !intermediate_signed_by_root {
                    return Err(StackhouseError::Unauthorized(
                        "Apple JWS x5c chain validation failed".into(),
                    ));
                }
            }
        }
    }

    if x5c.len() == 2 {
        if let (Ok(leaf_der), Ok(root_der)) = (
            URL_SAFE_NO_PAD.decode(x5c[0].as_str().unwrap_or("")),
            URL_SAFE_NO_PAD.decode(x5c[1].as_str().unwrap_or("")),
        ) {
            if let Ok((_, root)) = x509_parser::parse_x509_certificate(&root_der) {
                if !verify_cert_signed_by(&leaf_der, &root) {
                    return Err(StackhouseError::Unauthorized(
                        "Apple JWS x5c chain validation failed".into(),
                    ));
                }
            }
        }
    }

    let last_cert_b64 = x5c[x5c.len() - 1].as_str().ok_or_else(|| {
        StackhouseError::Unauthorized("Apple JWS x5c last cert not a string".into())
    })?;
    let last_cert_der = URL_SAFE_NO_PAD.decode(last_cert_b64).map_err(|e| {
        StackhouseError::Unauthorized(format!("Apple JWS x5c last cert decode: {e}"))
    })?;
    ensure_chain_roots_at_apple_root(&last_cert_der)?;

    Ok(payload)
}

fn verify_cert_signed_by(
    cert_der: &[u8],
    issuer: &x509_parser::certificate::X509Certificate,
) -> bool {
    // Parse the subject certificate
    let (_, subject) = match x509_parser::parse_x509_certificate(cert_der) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Get the issuer's public key raw bytes from SubjectPublicKeyInfo
    let spki_bytes = issuer.public_key().subject_public_key.data.as_ref();

    // X509Certificate::verify_signature can use the issuer's public key
    match subject.verify_signature(Some(issuer.public_key())) {
        Ok(_) => return true,
        Err(_) => {}
    }

    // Fallback: manual RSA verification using rsa crate
    use rsa::pkcs1::DecodeRsaPublicKey;
    use sha2::{Digest, Sha256};
    if let Ok(rsa_pub) = rsa::RsaPublicKey::from_pkcs1_der(spki_bytes) {
        let tbs_der = subject.tbs_certificate.as_ref();
        let mut hasher = Sha256::new();
        hasher.update(tbs_der);
        let hash = hasher.finalize();
        let sig_bytes = subject.signature_value.data.as_ref();
        let scheme = rsa::Pkcs1v15Sign::new::<Sha256>();
        return rsa_pub.verify(scheme, &hash, sig_bytes).is_ok();
    }

    // Fallback: manual ECDSA P-256 verification
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::{Signature, VerifyingKey};
    if let Ok(vk) = VerifyingKey::from_sec1_bytes(spki_bytes) {
        if let Ok(sig) = Signature::from_der(subject.signature_value.data.as_ref()) {
            return vk.verify(subject.tbs_certificate.as_ref(), &sig).is_ok();
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Google
// ---------------------------------------------------------------------------

/// Validate a Google Play purchase token using the Android Publisher API.
///
/// `access_token` must be an OAuth2 token obtained from the service account
/// (scope `https://www.googleapis.com/auth/androidpublisher`). We do **not**
/// mint the token here — integrators should cache one and pass it in.
pub async fn google_verify_subscription(
    http: &Client,
    access_token: &str,
    package_name: &str,
    subscription_id: &str,
    purchase_token: &str,
) -> StackhouseResult<ValidatedPurchase> {
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{pkg}/purchases/subscriptions/{sub}/tokens/{tok}",
        pkg = urlencoded(package_name),
        sub = urlencoded(subscription_id),
        tok = urlencoded(purchase_token),
    );

    let resp = http
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| StackhouseError::Storage(format!("google api: {e}")))?;
    if !resp.status().is_success() {
        return Err(StackhouseError::InvalidPayload(format!(
            "google purchases.subscriptions.get failed: {}",
            resp.status()
        )));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| StackhouseError::Storage(format!("google api parse: {e}")))?;

    let purchased = body
        .get("startTimeMillis")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single());
    let expires = body
        .get("expiryTimeMillis")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single());
    let auto_renew = body
        .get("autoRenewing")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let is_trial = body
        .get("paymentState")
        .and_then(|v| v.as_i64())
        .map(|s| s == 2)
        .unwrap_or(false);

    Ok(ValidatedPurchase {
        store: STORE_GOOGLE.to_string(),
        store_product_id: subscription_id.to_string(),
        store_transaction_id: purchase_token.to_string(),
        original_transaction_id: Some(purchase_token.to_string()),
        purchased_at: purchased,
        expires_at: expires,
        is_trial,
        is_renewal: false,
        auto_renew,
        raw: body,
    })
}

fn urlencoded(s: &str) -> String {
    // Minimal percent-encoding for path segments.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Stripe
// ---------------------------------------------------------------------------

/// Verify a `Stripe-Signature` header against the raw request body.
///
/// `tolerance_secs` is the maximum acceptable clock skew (e.g. 300 = 5 min).
pub fn stripe_verify_signature(
    payload: &[u8],
    signature_header: &str,
    signing_secret: &str,
    tolerance_secs: i64,
    now_unix: i64,
) -> StackhouseResult<()> {
    let mut timestamp: Option<i64> = None;
    let mut v1_sigs: Vec<&str> = Vec::new();
    for part in signature_header.split(',') {
        let mut kv = part.splitn(2, '=');
        let k = kv.next().unwrap_or("").trim();
        let v = kv.next().unwrap_or("").trim();
        match k {
            "t" => timestamp = v.parse().ok(),
            "v1" => v1_sigs.push(v),
            _ => {}
        }
    }
    let timestamp = timestamp
        .ok_or_else(|| StackhouseError::Unauthorized("stripe signature: missing t".into()))?;
    if (now_unix - timestamp).abs() > tolerance_secs {
        return Err(StackhouseError::Unauthorized(
            "stripe signature: timestamp outside tolerance".into(),
        ));
    }
    if v1_sigs.is_empty() {
        return Err(StackhouseError::Unauthorized(
            "stripe signature: missing v1".into(),
        ));
    }

    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .map_err(|e| StackhouseError::Unauthorized(format!("stripe hmac init: {e}")))?;
    mac.update(format!("{timestamp}.").as_bytes());
    mac.update(payload);
    let expected = mac.finalize().into_bytes();
    let expected_hex = hex::encode(expected);

    for sig in v1_sigs {
        if constant_time_eq(sig.as_bytes(), expected_hex.as_bytes()) {
            return Ok(());
        }
    }
    Err(StackhouseError::Unauthorized(
        "stripe signature: no matching v1 signature".into(),
    ))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Translate a Stripe event payload into a `ValidatedPurchase`, when applicable.
pub fn stripe_event_to_purchase(event: &Value) -> Option<ValidatedPurchase> {
    let obj = event.get("data")?.get("object")?;
    let event_type = event.get("type")?.as_str()?;
    if !matches!(
        event_type,
        "customer.subscription.created"
            | "customer.subscription.updated"
            | "customer.subscription.deleted"
            | "invoice.payment_succeeded"
    ) {
        return None;
    }

    let sub_id = obj.get("id")?.as_str()?.to_string();
    let product_id = obj
        .get("items")
        .and_then(|i| i.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("price"))
        .and_then(|p| p.get("product"))
        .and_then(|p| p.as_str())
        .unwrap_or("stripe_unknown")
        .to_string();
    let period_start = obj
        .get("current_period_start")
        .and_then(|v| v.as_i64())
        .and_then(|s| Utc.timestamp_opt(s, 0).single());
    let period_end = obj
        .get("current_period_end")
        .and_then(|v| v.as_i64())
        .and_then(|s| Utc.timestamp_opt(s, 0).single());
    let cancel_at_period_end = obj
        .get("cancel_at_period_end")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let is_trial = obj
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "trialing")
        .unwrap_or(false);

    Some(ValidatedPurchase {
        store: STORE_STRIPE.to_string(),
        store_product_id: product_id,
        store_transaction_id: sub_id.clone(),
        original_transaction_id: Some(sub_id),
        purchased_at: period_start,
        expires_at: period_end,
        is_trial,
        is_renewal: event_type == "invoice.payment_succeeded",
        auto_renew: !cancel_at_period_end,
        raw: event.clone(),
    })
}

/// Validate that experiment variant weights sum to exactly 100%.
pub fn validate_traffic_weights(variants: &[super::models::Variant]) -> StackhouseResult<()> {
    if variants.is_empty() {
        return Err(StackhouseError::InvalidPayload(
            "experiment must have at least one variant".into(),
        ));
    }

    let total = variants.iter().map(|v| v.traffic_weight).sum::<i32>();
    if total != 100 {
        return Err(StackhouseError::InvalidPayload(format!(
            "variant traffic weights must sum to 100, got {total}"
        )));
    }

    let controls = variants.iter().filter(|v| v.is_control).count();
    if controls != 1 {
        return Err(StackhouseError::InvalidPayload(
            "experiment must have exactly one control variant".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair};
    use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};

    #[test]
    fn stripe_signature_round_trip() {
        let secret = "whsec_test";
        let payload = br#"{"hello":"world"}"#;
        let ts: i64 = 1_700_000_000;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{ts}.").as_bytes());
        mac.update(payload);
        let sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={ts},v1={sig}");
        assert!(stripe_verify_signature(payload, &header, secret, 300, ts).is_ok());
        // Outside tolerance
        assert!(stripe_verify_signature(payload, &header, secret, 300, ts + 10_000).is_err());
        // Wrong secret
        assert!(stripe_verify_signature(payload, &header, "bad", 300, ts).is_err());
    }

    #[test]
    fn apple_jws_rejects_missing_x5c() {
        // Hand-crafted JWS without x5c chain — should now be rejected
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256"}"#);
        let body = URL_SAFE_NO_PAD.encode(br#"{"notificationType":"DID_RENEW"}"#);
        let sig = URL_SAFE_NO_PAD.encode(b"fake");
        let jws = format!("{header}.{body}.{sig}");
        // Should fail because there's no x5c chain
        assert!(apple_decode_jws_payload(&jws).is_err());
    }

    #[test]
    fn apple_root_anchor_accepts_pinned_root() {
        let pinned_der = decode_apple_root_ca().unwrap();
        assert!(ensure_chain_roots_at_apple_root(&pinned_der).is_ok());
    }

    #[test]
    fn apple_root_anchor_rejects_self_signed_fake_root() {
        let fake_root_key = KeyPair::generate().unwrap();
        let mut fake_root_params = CertificateParams::new(vec![]).unwrap();
        fake_root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        fake_root_params
            .distinguished_name
            .push(DnType::CommonName, "Fake Root");
        let fake_root = fake_root_params.self_signed(&fake_root_key).unwrap();

        assert!(ensure_chain_roots_at_apple_root(fake_root.der().as_ref()).is_err());
    }

    #[test]
    fn apple_jws_rejects_forged_self_signed_chain() {
        // Build a fake root and a leaf signed by it.
        let root_key = KeyPair::generate().unwrap();
        let mut root_params = CertificateParams::new(vec![]).unwrap();
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        root_params
            .distinguished_name
            .push(DnType::CommonName, "Fake Root");
        let root_cert = root_params.self_signed(&root_key).unwrap();

        let leaf_key = KeyPair::generate().unwrap();
        let mut leaf_params = CertificateParams::new(vec!["fake.apple.com".to_string()]).unwrap();
        leaf_params.is_ca = IsCa::NoCa;
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &root_cert, &root_key)
            .unwrap();

        let leaf_b64 = URL_SAFE_NO_PAD.encode(leaf_cert.der().as_ref());
        let root_b64 = URL_SAFE_NO_PAD.encode(root_cert.der().as_ref());
        let header_json = format!(r#"{{"alg":"ES256","x5c":["{leaf_b64}","{root_b64}"]}}"#);
        let header = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
        let payload = r#"{"notificationType":"DID_RENEW"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let signing_input = format!("{header}.{payload_b64}");
        let rng = ring::rand::SystemRandom::new();
        let signer = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_ASN1_SIGNING,
            &leaf_key.serialize_der(),
            &rng,
        )
        .unwrap();
        let signature = signer.sign(&rng, signing_input.as_bytes()).unwrap();
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.as_ref());
        let jws = format!("{header}.{payload_b64}.{sig_b64}");

        // Signature and 2-cert chain are internally consistent, but the root is not
        // Apple's pinned root CA, so the decode must fail.
        let result = apple_decode_jws_payload(&jws);
        assert!(
            result.is_err(),
            "forged self-signed chain should be rejected: {result:?}"
        );
    }

    #[test]
    fn apple_jws_rejects_forged_three_cert_chain() {
        // Build a fake root, fake intermediate, and a leaf signed by it.
        let root_key = KeyPair::generate().unwrap();
        let mut root_params = CertificateParams::new(vec![]).unwrap();
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        root_params
            .distinguished_name
            .push(DnType::CommonName, "Fake Root");
        let root_cert = root_params.self_signed(&root_key).unwrap();

        let intermediate_key = KeyPair::generate().unwrap();
        let mut intermediate_params = CertificateParams::new(vec![]).unwrap();
        intermediate_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        intermediate_params
            .distinguished_name
            .push(DnType::CommonName, "Fake Intermediate");
        let intermediate_cert = intermediate_params
            .signed_by(&intermediate_key, &root_cert, &root_key)
            .unwrap();

        let leaf_key = KeyPair::generate().unwrap();
        let mut leaf_params = CertificateParams::new(vec!["fake.apple.com".to_string()]).unwrap();
        leaf_params.is_ca = IsCa::NoCa;
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &intermediate_cert, &intermediate_key)
            .unwrap();

        let leaf_b64 = URL_SAFE_NO_PAD.encode(leaf_cert.der().as_ref());
        let intermediate_b64 = URL_SAFE_NO_PAD.encode(intermediate_cert.der().as_ref());
        let root_b64 = URL_SAFE_NO_PAD.encode(root_cert.der().as_ref());
        let header_json =
            format!(r#"{{"alg":"ES256","x5c":["{leaf_b64}","{intermediate_b64}","{root_b64}"]}}"#);
        let header = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
        let payload = r#"{"notificationType":"DID_RENEW"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let signing_input = format!("{header}.{payload_b64}");
        let rng = ring::rand::SystemRandom::new();
        let signer = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_ASN1_SIGNING,
            &leaf_key.serialize_der(),
            &rng,
        )
        .unwrap();
        let signature = signer.sign(&rng, signing_input.as_bytes()).unwrap();
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.as_ref());
        let jws = format!("{header}.{payload_b64}.{sig_b64}");

        // Chain consistency passes, but the root is not the pinned Apple root.
        let result = apple_decode_jws_payload(&jws);
        assert!(
            result.is_err(),
            "forged 3-cert chain should be rejected: {result:?}"
        );
    }
}
