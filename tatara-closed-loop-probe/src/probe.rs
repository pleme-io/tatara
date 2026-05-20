//! HTTP probe logic — pure data + tests for the parts that don't need
//! a live cluster.

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub service: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct ProbeConfig {
    pub issuer: ServiceEndpoint,
    pub issuer_auth_path: String,
    pub issuer_jwks_path: String,
    pub consumer: ServiceEndpoint,
    pub consumer_auth_path: String,
    pub access_id: String,
    pub access_key: String,
    pub http_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct ProbeOutput {
    pub intent_hash: String,
    pub artifact_hash: String,
    pub control_hash: String,
    pub issuer_url: String,
    pub consumer_url: String,
    pub token_present: bool,
    pub jwks_key_count: u64,
    pub whoami_status: u16,
}

pub async fn run(cfg: ProbeConfig) -> Result<ProbeOutput> {
    let issuer_base = [
        "http://",
        &cfg.issuer.service,
        ":",
        &cfg.issuer.port.to_string(),
    ]
    .concat();
    let consumer_base = [
        "http://",
        &cfg.consumer.service,
        ":",
        &cfg.consumer.port.to_string(),
    ]
    .concat();
    let issuer_auth_url = [issuer_base.as_str(), &cfg.issuer_auth_path].concat();
    let issuer_jwks_url = [issuer_base.as_str(), &cfg.issuer_jwks_path].concat();
    let consumer_auth_url = [consumer_base.as_str(), &cfg.consumer_auth_path].concat();

    let http = Client::builder()
        .timeout(cfg.http_timeout)
        .build()
        .context("build HTTP client")?;

    // 1. Authenticate against the issuer → token.
    let auth_resp = http
        .post(&issuer_auth_url)
        .json(&json!({
            "access-id": cfg.access_id,
            "access-key": cfg.access_key,
        }))
        .send()
        .await
        .with_context(|| format!("POST {issuer_auth_url}"))?;
    let auth_status = auth_resp.status();
    let auth_body = auth_resp.text().await.unwrap_or_default();
    if !auth_status.is_success() {
        return Err(anyhow!(
            "issuer auth failed: {auth_status} body={auth_body}"
        ));
    }
    let token = extract_token(&auth_body)?;
    let token_present = !token.is_empty();
    let artifact_hash = blake3_hex(token.as_bytes());

    // 2. Fetch JWKS for intent_hash.
    let jwks_resp = http
        .get(&issuer_jwks_url)
        .send()
        .await
        .with_context(|| format!("GET {issuer_jwks_url}"))?;
    let jwks_body = jwks_resp.text().await.unwrap_or_default();
    let jwks_key_count = count_jwks_keys(&jwks_body);
    let intent_hash = blake3_hex(jwks_body.as_bytes());

    // 3. Present the token to the consumer.
    let whoami_resp = http
        .post(&consumer_auth_url)
        .bearer_auth(&token)
        .send()
        .await
        .with_context(|| format!("POST {consumer_auth_url}"))?;
    let whoami_status = whoami_resp.status();
    let whoami_body = whoami_resp.text().await.unwrap_or_default();
    if !whoami_status.is_success() {
        return Err(anyhow!(
            "consumer rejected the issuer-issued token: {whoami_status} body={whoami_body}"
        ));
    }
    // control_hash = consumer's verdict
    let control_hash = blake3_hex(whoami_body.as_bytes());

    Ok(ProbeOutput {
        intent_hash,
        artifact_hash,
        control_hash,
        issuer_url: issuer_auth_url,
        consumer_url: consumer_auth_url,
        token_present,
        jwks_key_count,
        whoami_status: whoami_status.as_u16(),
    })
}

fn extract_token(body: &str) -> Result<String> {
    let v: Value = serde_json::from_str(body)
        .with_context(|| format!("parse issuer auth response: {body}"))?;
    // Akeyless returns `{ "token": "<jwt>" }` on success; tolerate
    // common variants without locking to a single shape.
    for field in ["token", "access_token", "auth_token", "jwt"] {
        if let Some(t) = v.get(field).and_then(|v| v.as_str()) {
            return Ok(t.to_string());
        }
    }
    Err(anyhow!("no token field found in issuer auth response"))
}

fn count_jwks_keys(body: &str) -> u64 {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("keys").cloned())
        .and_then(|k| k.as_array().map(|xs| xs.len() as u64))
        .unwrap_or(0)
}

fn blake3_hex(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_token_handles_known_fields() {
        for field in ["token", "access_token", "auth_token", "jwt"] {
            let body = json!({ field: "abc.def.ghi" }).to_string();
            let t = extract_token(&body).expect(field);
            assert_eq!(t, "abc.def.ghi");
        }
    }

    #[test]
    fn extract_token_errors_when_absent() {
        let body = json!({ "unrelated": "x" }).to_string();
        let err = extract_token(&body).unwrap_err();
        assert!(err.to_string().contains("no token field"));
    }

    #[test]
    fn count_jwks_keys_counts_keys_array() {
        let body = json!({
            "keys": [
                { "kty": "RSA", "kid": "1" },
                { "kty": "RSA", "kid": "2" },
                { "kty": "EC",  "kid": "3" },
            ]
        })
        .to_string();
        assert_eq!(count_jwks_keys(&body), 3);
    }

    #[test]
    fn count_jwks_keys_handles_missing_or_invalid() {
        assert_eq!(count_jwks_keys(""), 0);
        assert_eq!(count_jwks_keys("not json"), 0);
        assert_eq!(count_jwks_keys(r#"{"keys": "not-array"}"#), 0);
    }

    #[test]
    fn blake3_hex_is_deterministic() {
        assert_eq!(blake3_hex(b"hello"), blake3_hex(b"hello"));
        assert_ne!(blake3_hex(b"hello"), blake3_hex(b"world"));
    }
}
