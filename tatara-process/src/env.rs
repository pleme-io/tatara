//! `EphemeralEnvId` — the typed, validate-by-construction identity of an
//! ephemeral environment (the Dev-Loop "EnvId" keystone; Confluence "9 ·
//! Ephemeral Environments").
//!
//! A facade over the existing stringly derivation
//! ([`ephemeral_id_from_spec`](crate::hostname::ephemeral_id_from_spec)) — *same
//! hash, now a newtype* so an invalid id is **unrepresentable**: the only ways to
//! build one are [`from_spec`](EphemeralEnvId::from_spec) (derive — always valid,
//! idempotent: same spec ⇒ same id ⇒ same DNS slot) and
//! [`parse`](EphemeralEnvId::parse) (validate at an untrusted boundary —
//! parse-don't-validate). Deserialization routes through `parse`, so a malformed
//! id in a CRD/label/wire is **parse-time-rejected** (the eclusa §III.5 wire
//! discipline), never an in-flight value. Existing call sites keep using the
//! string fns unchanged; new typed code uses this newtype.

use serde::{Deserialize, Serialize};

use crate::hostname::{ephemeral_id_from_spec, HostnameError, EPHEMERAL_ID_HASH_LEN};

/// Why a string is not a valid [`EphemeralEnvId`].
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvIdError {
    #[error("ephemeral env id must be {EPHEMERAL_ID_HASH_LEN} chars, got {0}")]
    WrongLength(usize),
    #[error("ephemeral env id must be lowercase hex [0-9a-f], got {0:?}")]
    NotLowercaseHex(String),
}

/// The deterministic identity of an ephemeral environment — exactly
/// [`EPHEMERAL_ID_HASH_LEN`] lowercase-hex chars of BLAKE3 over a spec's
/// canonical JSON. Validate-by-construction (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct EphemeralEnvId(String);

impl EphemeralEnvId {
    /// Derive from a spec — the idempotent content-hash form (same value as
    /// [`ephemeral_id_from_spec`](crate::hostname::ephemeral_id_from_spec)). The
    /// derivation guarantees the invariant, so this only fails if the spec itself
    /// can't canonicalize.
    pub fn from_spec<T: Serialize>(spec: &T) -> Result<Self, HostnameError> {
        Ok(Self(ephemeral_id_from_spec(spec)?))
    }

    /// Parse + validate an id from an untrusted string (a CRD status, a label, an
    /// operator message) — the boundary where a bad id is rejected.
    pub fn parse(s: &str) -> Result<Self, EnvIdError> {
        if s.len() != EPHEMERAL_ID_HASH_LEN {
            return Err(EnvIdError::WrongLength(s.len()));
        }
        if !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(EnvIdError::NotLowercaseHex(s.to_string()));
        }
        Ok(Self(s.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EphemeralEnvId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EphemeralEnvId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for EphemeralEnvId {
    fn schema_name() -> String {
        "EphemeralEnvId".into()
    }
    fn json_schema(g: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut s = <String as schemars::JsonSchema>::json_schema(g);
        if let schemars::schema::Schema::Object(ref mut o) = s {
            o.string = Some(Box::new(schemars::schema::StringValidation {
                pattern: Some(format!("^[0-9a-f]{{{EPHEMERAL_ID_HASH_LEN}}}$")),
                min_length: Some(EPHEMERAL_ID_HASH_LEN as u32),
                max_length: Some(EPHEMERAL_ID_HASH_LEN as u32),
                ..Default::default()
            }));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::{EnvIdError, EphemeralEnvId};
    use crate::hostname::{ephemeral_id_from_spec, EPHEMERAL_ID_HASH_LEN};
    use serde::Serialize;

    #[derive(Serialize)]
    struct DummySpec {
        name: String,
        n: u32,
    }

    #[test]
    fn from_spec_is_deterministic_and_matches_the_raw_fn() {
        let spec = DummySpec { name: "gateway".into(), n: 7 };
        let a = EphemeralEnvId::from_spec(&spec).unwrap();
        let b = EphemeralEnvId::from_spec(&spec).unwrap();
        assert_eq!(a, b, "same spec ⇒ same id (idempotent)");
        // the newtype value IS the existing derivation
        assert_eq!(a.as_str(), ephemeral_id_from_spec(&spec).unwrap());
        assert_eq!(a.as_str().len(), EPHEMERAL_ID_HASH_LEN);
    }

    #[test]
    fn different_specs_yield_different_ids() {
        let a = EphemeralEnvId::from_spec(&DummySpec { name: "a".into(), n: 1 }).unwrap();
        let b = EphemeralEnvId::from_spec(&DummySpec { name: "a".into(), n: 2 }).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn parse_accepts_valid_and_rejects_invalid() {
        let valid = "0a1b2c3d"; // 8 lowercase-hex
        assert_eq!(EphemeralEnvId::parse(valid).unwrap().as_str(), valid);
        assert!(matches!(EphemeralEnvId::parse("0a1b").unwrap_err(), EnvIdError::WrongLength(4)));
        assert!(matches!(EphemeralEnvId::parse("0a1b2c3d4e").unwrap_err(), EnvIdError::WrongLength(10)));
        // uppercase / non-hex rejected
        assert!(matches!(EphemeralEnvId::parse("0A1B2C3D").unwrap_err(), EnvIdError::NotLowercaseHex(_)));
        assert!(matches!(EphemeralEnvId::parse("0a1b2c3z").unwrap_err(), EnvIdError::NotLowercaseHex(_)));
    }

    #[test]
    fn serde_round_trips_and_deserialize_rejects_malformed() {
        let id = EphemeralEnvId::parse("deadbeef").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"deadbeef\"");
        let back: EphemeralEnvId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
        // a malformed id in the wire is parse-time-rejected, never an in-Rust value
        assert!(serde_json::from_str::<EphemeralEnvId>("\"NOTHEX!!\"").is_err());
        assert!(serde_json::from_str::<EphemeralEnvId>("\"short\"").is_err());
    }
}
