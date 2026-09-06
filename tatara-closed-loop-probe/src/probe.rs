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

impl ServiceEndpoint {
    /// In-cluster HTTP base URL — `"http://{service}:{port}"` — the ONE
    /// substrate owner of the (`http://` scheme × service-DNS × port)
    /// composition that every closed-loop probe hand-authored at each
    /// endpoint bind pre-lift.
    ///
    /// Pre-lift the 4-slot
    /// `["http://", &<svc>.service, ":", &<svc>.port.to_string()].concat()`
    /// chain was open-coded at TWO adjacent sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold inside [`run`] — one
    /// for the issuer endpoint's base, one for the consumer endpoint's
    /// base — each restating the SAME 4-slot join to seed the base URL
    /// for the follow-on path-append composition through the
    /// sibling [`Self::url`] primitive.
    ///
    /// The `http://` scheme is baked into the primitive because the
    /// closed-loop probe operates inside the cluster where ClusterIP
    /// Services are reachable over plain HTTP; the ephemeral env's
    /// TLS boundary lives at the Ingress north-side, which the probe
    /// never crosses (see the CLAUDE.md **Ephemeral story** narrative
    /// on the closed-loop pattern: "runs ClusterIP + in-cluster HTTP
    /// between the bundled SaaS + Gateway; no Ingress, no per-namespace
    /// TLS"). A future closed-loop probe over an HTTPS service (a
    /// hypothetical externally-reachable SaaS whose issuer holds a
    /// public cert) lands as ONE scheme-slot injection at THIS
    /// substrate primitive — every downstream endpoint-URL consumer
    /// (issuer base, consumer base, plus every `url(<path>)` derivation)
    /// picks up the scheme upgrade mechanically without per-callsite
    /// hand-edit.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 4-slot HTTP-endpoint composition recurred at two hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger and is lifted to ONE method here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins below
    /// bind the primitive at fail-before-pass-after granularity, so a
    /// regression that dropped the port, drifted the scheme, or
    /// reshaped the concatenation surfaces at
    /// `tests::base_url_*` rather than as silent operator-facing skew
    /// between the two endpoint bases the probe issues requests
    /// against).
    #[must_use]
    pub fn base_url(&self) -> String {
        ["http://", &self.service, ":", &self.port.to_string()].concat()
    }

    /// Compose a path onto this endpoint's [`Self::base_url`] — the
    /// ONE substrate owner of the 2-slot
    /// `[<endpoint>.base_url().as_str(), <path>].concat()` chain
    /// hand-authored at THREE adjacent sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold inside [`run`]
    /// (issuer auth URL, issuer JWKS URL, consumer auth URL).
    ///
    /// The `path` slot is spliced verbatim — the primitive does NOT
    /// normalize a missing leading slash, collapse `//` boundaries, or
    /// canonicalize `.` / `..` segments — matching the pre-lift
    /// hand-authored `.concat()` posture byte-for-byte at every
    /// current callsite (the CLI defaults `/v2/auth`, `/.well-known/
    /// jwks.json`, `/v2/whoami` all carry a leading slash by
    /// convention). A future URL-canonicalization pass (a `url::Url::
    /// join`-style walk, a scheme-relative path handler) lands at
    /// this ONE substrate primitive and every downstream URL
    /// composition inherits the upgrade mechanically without
    /// per-callsite hand-edit.
    ///
    /// Peer to [`Self::base_url`] on the (base × base+path) axis:
    /// [`Self::base_url`] owns the scheme/host/port composition;
    /// this method composes on top of it, so a future scheme drift or
    /// port-slot upgrade reaches BOTH surfaces through the ONE
    /// composer chain.
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        [self.base_url().as_str(), path].concat()
    }
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
    // Endpoint URL composition rides the substrate primitives
    // [`ServiceEndpoint::base_url`] + [`ServiceEndpoint::url`] — pre-
    // lift these were FIVE hand-authored `.concat()` chains at this
    // site (TWO 4-slot `[http://, svc, :, port]` bases + THREE 2-slot
    // `[base, path]` derivations) past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold. Post-lift the endpoint→URL composition
    // lives at ONE substrate owner per axis; a future scheme drift
    // (HTTPS for externally-reachable probes) or URL-canonicalization
    // pass lands at THAT owner and both endpoints inherit the
    // upgrade mechanically.
    let issuer_auth_url = cfg.issuer.url(&cfg.issuer_auth_path);
    let issuer_jwks_url = cfg.issuer.url(&cfg.issuer_jwks_path);
    let consumer_auth_url = cfg.consumer.url(&cfg.consumer_auth_path);

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
    let artifact_hash = tatara_process::hash::hex_blake3(token.as_bytes());

    // 2. Fetch JWKS for intent_hash.
    let jwks_resp = http
        .get(&issuer_jwks_url)
        .send()
        .await
        .with_context(|| format!("GET {issuer_jwks_url}"))?;
    let jwks_body = jwks_resp.text().await.unwrap_or_default();
    let jwks_key_count = count_jwks_keys(&jwks_body);
    let intent_hash = tatara_process::hash::hex_blake3(jwks_body.as_bytes());

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
    let control_hash = tatara_process::hash::hex_blake3(whoami_body.as_bytes());

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
    // The reference issuer returns `{ "token": "<jwt>" }` on success; tolerate
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

    // ─── ServiceEndpoint URL composition substrate pins ─────────────
    //
    // These pins bind the substrate primitives [`ServiceEndpoint::
    // base_url`] + [`ServiceEndpoint::url`] at fail-before-pass-after
    // granularity. Pre-lift the 4-slot HTTP-base composition and its
    // 2-slot path-append derivation recurred at FIVE hand-authored
    // sites inside [`run`] past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold. Post-lift the composition lives at ONE substrate
    // owner per axis; a regression that dropped the port slot,
    // drifted the scheme, or reshaped the concatenation surfaces
    // HERE rather than as silent operator-facing skew across the
    // issuer + consumer endpoints the probe issues live requests
    // against.

    #[test]
    fn base_url_composes_http_scheme_service_port_verbatim() {
        // The primitive composes exactly `"http://{service}:{port}"`
        // — no trailing slash, no path segment, no scheme drift. A
        // regression that swapped in `"https://"` (a widened scheme
        // slot without an in-scope typed injection), inserted an
        // implicit trailing slash (breaking downstream path append),
        // or reshaped the port formatting (leading zeros, hex output)
        // surfaces HERE rather than at the wire request. Covers the
        // two live shapes: a common issuer port (8080) + a common
        // consumer port (8000) matching the CLI defaults in main.rs.
        let issuer = ServiceEndpoint {
            service: "issuer".into(),
            port: 8080,
        };
        assert_eq!(issuer.base_url(), "http://issuer:8080");
        let consumer = ServiceEndpoint {
            service: "gateway".into(),
            port: 8000,
        };
        assert_eq!(consumer.base_url(), "http://gateway:8000");
    }

    #[test]
    fn base_url_matches_pre_lift_hand_authored_concat_chain_bytewise() {
        // Byte-identical parity pin: for every observable (service,
        // port) input, the primitive produces the SAME `String` a
        // pre-lift 4-slot `["http://", &<svc>.service, ":",
        // &<svc>.port.to_string()].concat()` chain produced. A
        // regression that reshaped the internal concatenation (e.g.
        // switched to `format!("http://{}:{}")` — safe on the wire
        // but a category slip vs the pre-lift `.concat()` posture)
        // still passes only because it observably produces the same
        // bytes; the pin fixes the OBSERVABLE contract.
        for (service, port) in [
            ("issuer", 8080u16),
            ("gateway", 8000),
            ("saas.observability", 1),
            ("s", u16::MAX),
        ] {
            let ep = ServiceEndpoint {
                service: service.into(),
                port,
            };
            let pre_lift = ["http://", &ep.service, ":", &ep.port.to_string()].concat();
            assert_eq!(
                ep.base_url(),
                pre_lift,
                "base_url() for ({service:?}, {port}) must match pre-lift 4-slot .concat()"
            );
        }
    }

    #[test]
    fn url_appends_path_to_base_url_verbatim() {
        // The primitive splices `path` verbatim onto `base_url` — no
        // leading-slash normalization, no `//` collapse, no percent-
        // encoding. Covers the three live path shapes threading
        // through `run(cfg)`: `--issuer-auth-path` (`/v2/auth`),
        // `--issuer-jwks-path` (`/.well-known/jwks.json`),
        // `--consumer-auth-path` (`/v2/whoami`) — all lead with `/`
        // by CLI-default convention. A regression that stripped a
        // leading slash, normalized `.well-known`, or dropped a
        // subpath segment surfaces here rather than as an operator-
        // facing 404 at the wire.
        let ep = ServiceEndpoint {
            service: "issuer".into(),
            port: 8080,
        };
        assert_eq!(ep.url("/v2/auth"), "http://issuer:8080/v2/auth");
        assert_eq!(
            ep.url("/.well-known/jwks.json"),
            "http://issuer:8080/.well-known/jwks.json"
        );
        assert_eq!(ep.url("/v2/whoami"), "http://issuer:8080/v2/whoami");
        // Preserves caller's leading-slash discipline: a slashless
        // path composes with byte-identical `.concat()` semantics —
        // NOT what URL-canonical joins do, but what the pre-lift
        // `.concat()` chain did, so the pin binds the current
        // contract byte-for-byte.
        assert_eq!(ep.url("v2/auth"), "http://issuer:8080v2/auth");
        // Empty path returns the base URL unchanged.
        assert_eq!(ep.url(""), "http://issuer:8080");
    }

    #[test]
    fn url_matches_pre_lift_hand_authored_concat_chain_bytewise() {
        // Byte-identical parity pin: for every observable (endpoint,
        // path) input, the primitive produces the SAME `String` a
        // pre-lift 2-slot `[<ep>.base_url().as_str(),
        // &<path>].concat()` chain produced. Sibling to the
        // `base_url_matches_pre_lift_*` pin above — together the two
        // pins bind the (base × base+path) axis pair the two
        // substrate primitives close.
        for (service, port, path) in [
            ("issuer", 8080u16, "/v2/auth"),
            ("issuer", 8080, "/.well-known/jwks.json"),
            ("gateway", 8000, "/v2/whoami"),
            ("s", 1, ""),
            ("s", 1, "path"),
        ] {
            let ep = ServiceEndpoint {
                service: service.into(),
                port,
            };
            let pre_lift = [ep.base_url().as_str(), path].concat();
            assert_eq!(
                ep.url(path),
                pre_lift,
                "url({path:?}) for ({service:?}, {port}) must match pre-lift 2-slot .concat()"
            );
        }
    }
}
