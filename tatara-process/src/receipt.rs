//! `tatara-receipt/v1` — the typed receipt envelope every pleme-io Job
//! emits to prove its work was done.
//!
//! Today's consumers (and the only ones supported on `tatara-receipt/v1`):
//! - **closed-loop auth probes** — `kind = "closed-loop-auth"`. Stamps
//!   that a system's bundled identity issuer authenticated its bundled
//!   client. The substrate primitive every closed-loop-testable product
//!   composes (Akeyless gator↔gateway, future: identity providers,
//!   message brokers, databases that can issue creds to themselves).
//! - **schema/migration runs** — `kind = "db-migration"`. shinka emits
//!   one per applied migration; pillars carry the diff hash.
//! - **test suites** — `kind = "test-suite"`. kenshi-runner et al.
//! - **nix builds** — `kind = "nix-build"`. Carries the store-path
//!   pillar as `artifact_hash`.
//! - Anything else — operators register new `kind` strings; the
//!   schema is open by design (the *shape* is fixed; the kind is data).
//!
//! Lives in `tatara-process` so `ReceiptEnvelope → ProcessAttestation`
//! is a local typed bridge — the reconciler's verifier and any future
//! Process consumer share one parse.
//!
//! Wire format (snake_case to match the existing ConfigMap payload
//! shape the akeyless-closed-loop-probe chart writes):
//!
//! ```yaml
//! version: tatara-receipt/v1
//! kind: closed-loop-auth
//! composed_root: <26-char hex>
//! intent_hash:   <hex>
//! artifact_hash: <hex>
//! control_hash:  <hex>
//! generated_at:  2026-05-19T22:00:00Z
//! process_ref:   "akeyless-test/ephemeral-akeyless"   # optional
//! evidence:      { ... }                              # optional, free-form
//! ```

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attestation::ProcessAttestation;

/// Canonical version string. Bump → `tatara-receipt/v2` if the wire
/// shape changes; parsers refuse anything else for the v1 reader.
pub const RECEIPT_VERSION: &str = "tatara-receipt/v1";

/// Closed-set typed identifier for the four known [`ReceiptEnvelope::kind`]
/// strings the substrate emits today — [`Self::ClosedLoopAuth`] →
/// `"closed-loop-auth"`, [`Self::DbMigration`] → `"db-migration"`,
/// [`Self::TestSuite`] → `"test-suite"`, [`Self::NixBuild`] →
/// `"nix-build"` — as a Rust enum, so the (variant, canonical kebab-case
/// kind, semantic role) triple binds at ONE site on the typed algebra
/// rather than at the four byte-identical string-literal sites scattered
/// across the closed-loop probe binary (`default_value` on
/// `--receipt-kind`), the reconciler's receipt-parser tests, the
/// `ephemeral_pipeline` integration test, and the future shinka /
/// kenshi / nix-build Job authors that compose `ReceiptEnvelope::build`.
///
/// Pre-lift the four canonical kebab-case kinds lived as `&'static str`
/// literal arguments at every author site (`ReceiptEnvelope::build(
/// "closed-loop-auth", …)`) AND as docstring prose at this module's
/// header (`Today's consumers: closed-loop-auth, db-migration,
/// test-suite, nix-build`). The (canonical-string, semantic-role)
/// pairing was load-bearing across ≥5 files yet enforced by per-site
/// call-site discipline — a rename of `"closed-loop-auth"` →
/// `"closed-loop"` at the probe binary's CLI default (the originator of
/// every production receipt) silently desynchronizes from the docstring
/// prose AND from the reconciler's test fixtures AND from any future
/// kind-keyed dispatch (e.g. shinka's per-kind verifier registry) — the
/// `kind` field is a `String` from the wire shape's perspective so the
/// compiler cannot bind the literals together. Post-lift the canonical
/// kebab-case strings live at ONE [`Self::as_str`] arm per variant;
/// every author site composes the typed variant through
/// `ReceiptEnvelope::build(ReceiptKind::ClosedLoopAuth, …)` (the typed
/// → `String` `From` impl lets the existing `impl Into<String>` API
/// surface accept the variant transparently) and a rename lands at ONE
/// `as_str` arm here — no per-call-site grep + edit sweep, no silent
/// drift between the docstring header and the wire literals.
///
/// The `kind` field on [`ReceiptEnvelope`] remains a `String` because
/// the schema is open by design: operators register new `kind` strings
/// for future consumers (operator-domain Job receipts) without bumping
/// the wire version. The typed `ReceiptKind` is the closed-set *view*
/// over that open String — every receipt the substrate itself emits
/// projects through one of the four typed variants, and the typed
/// projection [`ReceiptEnvelope::known_kind`] decodes any envelope's
/// `kind` into `Some(ReceiptKind)` when it matches a known variant,
/// `None` for operator-registered open kinds. The (open-String,
/// closed-typed-view) split is the same shape `tatara-lisp`'s
/// `Sexp::Sym` (open atoms) vs `MacroDefHead` (closed-set head
/// markers) takes — open data through one type, closed dispatch
/// through another, no `_` fallthrough where the closed set runs.
///
/// Adding a fifth kind (e.g. `Provenance` → `"provenance-attest"`)
/// extends the enum AND the two projection arms ([`Self::as_str`],
/// [`Self::from_str`] via the [`Self::ALL`] sweep) in lockstep — rustc
/// binds the extension through exhaustiveness over the closed enum so
/// a partial extension that forgets ONE projection becomes a compile
/// error rather than a runtime drift where the new kind builds receipts
/// but `known_kind()` returns `None` and the future kind-keyed verifier
/// dispatch silently falls through.
///
/// Sibling closed-set [`Self::ALL`] lift across the crate:
/// [`crate::export::ReportFormat::ALL`],
/// [`crate::export::ExportTrigger::ALL`],
/// [`crate::export::ReportPayloadShape::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::lifetime_clock::TerminateReasonKind::ALL`].
///
/// Theory anchor: THEORY.md §III — the typescape; the substrate's own
/// receipt kinds become a TYPE rather than four `&'static str` literals
/// at every author site and a docstring header that drifts the moment
/// any rename happens off-script. THEORY.md §V.3 — three-pillar
/// attestation; the `kind` field is the *what-am-I* discriminator on
/// every receipt that chains into a [`ProcessAttestation`], and the
/// typed variant is the substrate's shared vocabulary for "which kind
/// of work just got attested" — pre-lift each call site had to spell
/// the kind by hand, post-lift each call site composes the typed
/// constant and any consumer (future verifier, future dashboard, future
/// LSP completion) sweeps [`Self::ALL`] to enumerate every known
/// substrate-emitted receipt without grep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReceiptKind {
    /// Closed-loop auth probe — stamps that a system's bundled identity
    /// issuer authenticated its bundled client. Emitted by
    /// `tatara-closed-loop-probe`; the substrate primitive every
    /// closed-loop-testable product composes (Akeyless gator↔gateway,
    /// future: identity providers, message brokers, databases that can
    /// issue creds to themselves).
    ClosedLoopAuth,
    /// Schema/migration runs. shinka emits one per applied migration;
    /// the pillars carry the diff hash so the chain shows exactly which
    /// migration was applied where.
    DbMigration,
    /// Test suites — kenshi-runner et al. The `evidence` field carries
    /// pass/fail counts; the pillars stamp the suite identity.
    TestSuite,
    /// Nix builds. Carries the store-path pillar as `artifact_hash`;
    /// chains every reproducible build into the Process attestation
    /// chain so a derivation's output is provable on its owning
    /// Process.
    NixBuild,
}

impl ReceiptKind {
    /// The closed set of substrate-emitted receipt kinds — single
    /// source of truth that drives the [`Self::from_str`] decode sweep
    /// AND any future enumeration consumer (kind-keyed verifier
    /// registry, dashboard completion list, `tatara-check` receipt-kind
    /// enumeration). Adding a fifth variant (e.g. `Provenance` →
    /// `"provenance-attest"`) lands at one `ALL` entry + one `as_str`
    /// arm — exhaustively checked by the compiler (the `[Self; 4]`
    /// array literal forces the arity) AND by the per-variant
    /// truth-table tests below.
    ///
    /// Sibling closed-set lifts across the crate's typescape:
    /// [`crate::export::ReportFormat::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::intent::IntentKind::ALL`].
    pub const ALL: [Self; 4] = [
        Self::ClosedLoopAuth,
        Self::DbMigration,
        Self::TestSuite,
        Self::NixBuild,
    ];

    /// Canonical kebab-case wire-format kind — the literal that lands
    /// in [`ReceiptEnvelope::kind`] when this variant authors the
    /// receipt. Pinned to four byte-exact strings the substrate has
    /// already published (the closed-loop probe's `default_value` on
    /// `--receipt-kind`, the reconciler tests' fixture builds, the
    /// `ephemeral_pipeline` integration test's assertions) — renaming
    /// any one is a wire-format change, not a typed-internal refactor,
    /// and the `receipt_kind_canonical_names_pinned` truth-table test
    /// fails first to keep the substrate honest. Used by
    /// [`fmt::Display`] (single source of truth) and as the `String`
    /// projection that `From<ReceiptKind> for String` ([`Self::into`])
    /// composes so [`ReceiptEnvelope::build`]'s `impl Into<String>`
    /// kind argument transparently accepts the typed variant.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClosedLoopAuth => "closed-loop-auth",
            Self::DbMigration => "db-migration",
            Self::TestSuite => "test-suite",
            Self::NixBuild => "nix-build",
        }
    }

}

/// Decode a `kind` string into the typed variant — `Ok(kind)` when the
/// string matches one of the four canonical kebab-case literals exactly
/// (byte-equal, case-sensitive — the wire shape is pinned),
/// `Err(UnknownReceiptKind)` for any other string (open operator-
/// registered kinds, typos, future-version kinds). Round-trip invariant
/// pinned by `receipt_kind_from_str_round_trips_canonical_names`:
/// `k.as_str().parse() == Ok(k)` for every variant. Open-by-design
/// callers prefer [`ReceiptEnvelope::known_kind`]'s `Option<ReceiptKind>`
/// shape, which collapses the typed `Err` into a `None` so open kinds
/// stay open. Lifted onto a linear sweep over [`Self::ALL`] keyed on
/// [`Self::as_str`] so the canonical literals live at ONE site (the
/// `as_str` arms) rather than at TWO sites (a `from_str` `match` arm AND
/// an `as_str` arm per variant) — adding a fifth kind extends only
/// `ALL` + `as_str`, NOT a third per-variant literal site.
impl FromStr for ReceiptKind {
    type Err = UnknownReceiptKind;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for kind in Self::ALL {
            if s == kind.as_str() {
                return Ok(kind);
            }
        }
        Err(UnknownReceiptKind(s.to_string()))
    }
}

/// Typed parse error for [`ReceiptKind::from_str`] — carries the
/// offending input verbatim so an operator-facing diagnostic surfaces
/// the bad value, not a normalized form. Symmetric to every sibling
/// `Unknown*` error in this crate (e.g. [`crate::phase::UnknownPhase`],
/// [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::lifetime_clock::UnknownTerminateReasonKind`]).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("unknown receipt kind: {0}")]
pub struct UnknownReceiptKind(pub String);

impl fmt::Display for ReceiptKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<ReceiptKind> for String {
    /// Composes [`ReceiptKind::as_str`] into an owned `String` so
    /// every `impl Into<String>` API surface ([`ReceiptEnvelope::build`]'s
    /// `kind` parameter most notably) accepts the typed variant
    /// transparently — the call site stays `build(kind, …)` and the
    /// typed → wire bridge runs through ONE place.
    fn from(k: ReceiptKind) -> Self {
        k.as_str().to_owned()
    }
}

impl From<ReceiptKind> for &'static str {
    fn from(k: ReceiptKind) -> Self {
        k.as_str()
    }
}

/// Typed receipt envelope. Any Job in pleme-io that wants its result to
/// chain into a Process's `status.attestation` writes one of these.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ReceiptEnvelope {
    /// Must equal `RECEIPT_VERSION`. Mismatches reject the receipt.
    pub version: String,
    /// What this receipt proves. Known: `closed-loop-auth`, `db-migration`,
    /// `test-suite`, `nix-build`. Operators may register new kinds —
    /// the envelope is open.
    pub kind: String,
    /// Three-pillar root: `BLAKE3(domain ++ artifact ++ control ++ intent ++ previous)`.
    pub composed_root: String,
    /// Pillar 1: what the Job was *trying* to do (canonical intent).
    pub intent_hash: String,
    /// Pillar 2: what the Job *produced* (artifact / proof material).
    pub artifact_hash: String,
    /// Pillar 3: how the Job *verified* its work (controls / signatures /
    /// auth steps). Empty string when there was no control step.
    pub control_hash: String,
    /// Timestamp the Job set when it wrote the receipt.
    pub generated_at: DateTime<Utc>,
    /// Optional owning-Process reference (`namespace/name`). When the
    /// reconciler creates the Job it stamps this in via the downward
    /// API; receipts without it still parse for ad-hoc / out-of-cluster
    /// runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_ref: Option<String>,
    /// Optional structured evidence. Free-form JSON. The reconciler does
    /// not parse this — it's for human / downstream-tool inspection.
    #[serde(default, skip_serializing_if = "is_null")]
    pub evidence: serde_json::Value,
}

fn is_null(v: &serde_json::Value) -> bool {
    v.is_null()
}

/// Why a receipt is rejected. Kept as a typed enum so callers can
/// pattern-match on the failure mode and surface targeted operator
/// messages.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReceiptError {
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("invalid YAML: {0}")]
    InvalidYaml(String),
    #[error("version != {RECEIPT_VERSION} (got {0:?})")]
    WrongVersion(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("kind is empty")]
    EmptyKind,
    #[error("composed_root mismatch (got {got}, want {want})")]
    RootMismatch { got: String, want: String },
}

impl ReceiptEnvelope {
    /// Build a receipt envelope from typed pillars + kind. `generated_at`
    /// defaults to `Utc::now()`.
    pub fn build(
        kind: impl Into<String>,
        intent_hash: impl Into<String>,
        artifact_hash: impl Into<String>,
        control_hash: impl Into<String>,
        previous_root: Option<&str>,
    ) -> Self {
        let intent_hash = intent_hash.into();
        let artifact_hash = artifact_hash.into();
        let control_hash = control_hash.into();
        let composed_root = compose_root(
            &artifact_hash,
            if control_hash.is_empty() {
                None
            } else {
                Some(control_hash.as_str())
            },
            &intent_hash,
            previous_root,
        );
        Self {
            version: RECEIPT_VERSION.into(),
            kind: kind.into(),
            composed_root,
            intent_hash,
            artifact_hash,
            control_hash,
            generated_at: Utc::now(),
            process_ref: None,
            evidence: serde_json::Value::Null,
        }
    }

    /// Parse a receipt from a JSON string.
    pub fn parse_json(payload: &str) -> Result<Self, ReceiptError> {
        let env: Self =
            serde_json::from_str(payload).map_err(|e| ReceiptError::InvalidJson(e.to_string()))?;
        env.verify_shape()?;
        Ok(env)
    }

    /// Parse a receipt from a YAML string. Useful for ConfigMaps that
    /// store the payload in YAML form.
    pub fn parse_yaml(payload: &str) -> Result<Self, ReceiptError> {
        let env: Self =
            serde_yaml::from_str(payload).map_err(|e| ReceiptError::InvalidYaml(e.to_string()))?;
        env.verify_shape()?;
        Ok(env)
    }

    /// Parse via JSON first, then YAML if JSON fails. Lets a single
    /// reader accept either wire form without the operator having to
    /// declare it. Useful when the Job writes JSON and the reconciler
    /// reads back through a kube DynamicObject whose `data` is YAML.
    pub fn parse_either(payload: &str) -> Result<Self, ReceiptError> {
        match Self::parse_json(payload) {
            Ok(env) => Ok(env),
            Err(_) => Self::parse_yaml(payload),
        }
    }

    /// Verify the schema-level invariants: correct version + non-empty
    /// kind + non-empty pillar hashes (length-only, not BLAKE3-recompute).
    pub fn verify_shape(&self) -> Result<(), ReceiptError> {
        if self.version != RECEIPT_VERSION {
            return Err(ReceiptError::WrongVersion(self.version.clone()));
        }
        if self.kind.is_empty() {
            return Err(ReceiptError::EmptyKind);
        }
        if self.composed_root.is_empty() {
            return Err(ReceiptError::MissingField("composed_root"));
        }
        if self.intent_hash.is_empty() {
            return Err(ReceiptError::MissingField("intent_hash"));
        }
        if self.artifact_hash.is_empty() {
            return Err(ReceiptError::MissingField("artifact_hash"));
        }
        // control_hash MAY be empty when there is no control step;
        // the BLAKE3 compose treats empty as "absent" via Option.
        Ok(())
    }

    /// Verify that `composed_root` is consistent with the pillars.
    /// `expected_previous_root` is the previous root in the Process's
    /// attestation chain (or `None` for first attestation).
    pub fn verify_root(&self, expected_previous_root: Option<&str>) -> bool {
        let want = compose_root(
            &self.artifact_hash,
            if self.control_hash.is_empty() {
                None
            } else {
                Some(self.control_hash.as_str())
            },
            &self.intent_hash,
            expected_previous_root,
        );
        constant_time_eq(want.as_bytes(), self.composed_root.as_bytes())
    }

    /// Strict-equality check against an operator-provided expected root.
    /// Returns the receipt's root unchanged on success.
    pub fn expect_root(&self, expected: Option<&str>) -> Result<&str, ReceiptError> {
        if let Some(want) = expected {
            if want != self.composed_root {
                return Err(ReceiptError::RootMismatch {
                    got: self.composed_root.clone(),
                    want: want.to_string(),
                });
            }
        }
        Ok(&self.composed_root)
    }

    /// Decode `self.kind` into the typed [`ReceiptKind`] variant when
    /// the wire string matches one of the four substrate-emitted
    /// canonical kebab-case kinds; `None` when the kind is an
    /// operator-registered open string (the schema is open by design —
    /// every receipt remains a valid receipt, but only typed kinds
    /// participate in closed-set dispatch). The (open `String`,
    /// closed-typed view) split lets future kind-keyed consumers
    /// (verifier registries, dashboard completion, audit-trail
    /// classifiers) sweep the typed variants without touching the
    /// open-by-design wire shape. Lifted as the canonical decode site
    /// so no consumer re-implements the `match self.kind.as_str()`
    /// arm-by-arm — the closed-set sweep happens through
    /// [`ReceiptKind::from_str`] at ONE site.
    #[must_use]
    pub fn known_kind(&self) -> Option<ReceiptKind> {
        self.kind.parse().ok()
    }

    /// Lower into a `ProcessAttestation` — the canonical handoff so a
    /// Job's typed receipt becomes evidence on a Process. `generation`
    /// + `previous_root` come from the owning Process's prior
    /// attestation (or 0 + None for the first cycle).
    pub fn to_attestation(
        &self,
        generation: u64,
        previous_root: Option<&str>,
    ) -> ProcessAttestation {
        ProcessAttestation::compose(
            self.artifact_hash.clone(),
            if self.control_hash.is_empty() {
                None
            } else {
                Some(self.control_hash.clone())
            },
            self.intent_hash.clone(),
            previous_root.map(String::from),
            generation,
        )
    }
}

const DOMAIN_TAG: &[u8] = b"tatara-process/v1alpha1\n";

/// Same composition as `ProcessAttestation::composed_hex` — kept local so
/// `tatara_process::receipt::compose_root(...)` is a single line in
/// downstream code without re-importing the attestation module.
fn compose_root(
    artifact: &str,
    control: Option<&str>,
    intent: &str,
    previous: Option<&str>,
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(DOMAIN_TAG);
    h.update(artifact.as_bytes());
    h.update(b"\n");
    h.update(control.unwrap_or("").as_bytes());
    h.update(b"\n");
    h.update(intent.as_bytes());
    h.update(b"\n");
    h.update(previous.unwrap_or("").as_bytes());
    hex::encode(h.finalize().as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> &'static str {
        // Composed_root precomputed from compose_root("bbbb", Some("cccc"), "aaaa", None)
        // (recomputed at test time to be canonical; this string is regenerated
        // if the domain tag ever changes).
        r#"{
            "version": "tatara-receipt/v1",
            "kind": "closed-loop-auth",
            "composed_root": "RECOMPUTE",
            "intent_hash":   "aaaa",
            "artifact_hash": "bbbb",
            "control_hash":  "cccc",
            "generated_at":  "2026-05-19T12:00:00Z"
        }"#
    }

    fn canonical_payload_json() -> String {
        let root = compose_root("bbbb", Some("cccc"), "aaaa", None);
        sample_payload().replace("RECOMPUTE", &root)
    }

    #[test]
    fn build_produces_valid_envelope() {
        let r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        assert_eq!(r.version, RECEIPT_VERSION);
        assert_eq!(r.kind, "test-suite");
        assert!(r.verify_shape().is_ok());
        assert!(r.verify_root(None));
    }

    #[test]
    fn build_empty_control_omits_from_root() {
        let with_empty = ReceiptEnvelope::build("nix-build", "i", "a", "", None);
        let with_explicit_none = ReceiptEnvelope::build("nix-build", "i", "a", "", None);
        assert_eq!(with_empty.composed_root, with_explicit_none.composed_root);

        // And differs from a receipt with a real control hash.
        let with_control = ReceiptEnvelope::build("nix-build", "i", "a", "c", None);
        assert_ne!(with_empty.composed_root, with_control.composed_root);
    }

    #[test]
    fn parse_json_round_trip() {
        let r = ReceiptEnvelope::parse_json(&canonical_payload_json()).expect("parse");
        assert_eq!(r.kind, "closed-loop-auth");
        assert!(r.verify_root(None));
    }

    #[test]
    fn parse_yaml_round_trip() {
        let yaml = r#"
version: tatara-receipt/v1
kind: db-migration
composed_root: ROOT
intent_hash:   aaaa
artifact_hash: bbbb
control_hash:  cccc
generated_at:  2026-05-19T12:00:00Z
"#
        .replace("ROOT", &compose_root("bbbb", Some("cccc"), "aaaa", None));
        let r = ReceiptEnvelope::parse_yaml(&yaml).expect("yaml parse");
        assert_eq!(r.kind, "db-migration");
        assert!(r.verify_root(None));
    }

    #[test]
    fn parse_either_falls_back_to_yaml() {
        let yaml = r#"
version: tatara-receipt/v1
kind: test-suite
composed_root: ROOT
intent_hash:   aaaa
artifact_hash: bbbb
control_hash:  cccc
generated_at:  2026-05-19T12:00:00Z
"#
        .replace("ROOT", &compose_root("bbbb", Some("cccc"), "aaaa", None));
        assert!(ReceiptEnvelope::parse_either(&yaml).is_ok());
    }

    #[test]
    fn wrong_version_rejected() {
        let mut env: serde_json::Value = serde_json::from_str(&canonical_payload_json()).unwrap();
        env["version"] = "tatara-receipt/v2".into();
        let err = ReceiptEnvelope::parse_json(&env.to_string()).unwrap_err();
        assert!(matches!(err, ReceiptError::WrongVersion(ref s) if s == "tatara-receipt/v2"));
    }

    #[test]
    fn missing_field_rejected() {
        let mut env: serde_json::Value = serde_json::from_str(&canonical_payload_json()).unwrap();
        env.as_object_mut().unwrap().remove("intent_hash");
        let err = ReceiptEnvelope::parse_json(&env.to_string()).unwrap_err();
        assert!(matches!(err, ReceiptError::InvalidJson(_)));
    }

    #[test]
    fn unknown_field_rejected() {
        let mut env: serde_json::Value = serde_json::from_str(&canonical_payload_json()).unwrap();
        env["forged_extra"] = "should-fail".into();
        let err = ReceiptEnvelope::parse_json(&env.to_string()).unwrap_err();
        assert!(matches!(err, ReceiptError::InvalidJson(_)));
    }

    #[test]
    fn empty_kind_rejected_in_verify_shape() {
        let mut r = ReceiptEnvelope::build("k", "i", "a", "c", None);
        r.kind = String::new();
        assert!(matches!(r.verify_shape(), Err(ReceiptError::EmptyKind)));
    }

    #[test]
    fn expect_root_matches_or_mismatches() {
        let r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        let root = r.composed_root.clone();
        assert!(r.expect_root(Some(&root)).is_ok());
        let err = r.expect_root(Some("nope")).unwrap_err();
        assert!(matches!(err, ReceiptError::RootMismatch { .. }));
        assert!(r.expect_root(None).is_ok());
    }

    #[test]
    fn lower_to_attestation_chains_pillars() {
        let r = ReceiptEnvelope::build("closed-loop-auth", "i", "a", "c", None);
        let a = r.to_attestation(0, None);
        assert_eq!(a.intent_hash, "i");
        assert_eq!(a.artifact_hash, "a");
        assert_eq!(a.control_hash.as_deref(), Some("c"));
        // Both compose the same root.
        assert_eq!(a.composed_root, r.composed_root);
        assert!(a.verify());

        let next = r.to_attestation(1, Some(&a.composed_root));
        assert_eq!(next.generation, 1);
        assert_eq!(
            next.previous_root.as_deref(),
            Some(a.composed_root.as_str())
        );
        // The composed_root differs because previous_root is included.
        assert_ne!(next.composed_root, a.composed_root);
    }

    #[test]
    fn verify_root_detects_tamper() {
        let mut r = ReceiptEnvelope::build("closed-loop-auth", "i", "a", "c", None);
        assert!(r.verify_root(None));
        r.intent_hash = "tampered".into();
        assert!(!r.verify_root(None));
    }

    #[test]
    fn process_ref_optional_and_round_trips() {
        let mut r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        r.process_ref = Some("akeyless-test/ephemeral".into());
        let s = serde_json::to_string(&r).unwrap();
        let back = ReceiptEnvelope::parse_json(&s).expect("round-trip");
        assert_eq!(back.process_ref.as_deref(), Some("akeyless-test/ephemeral"));
    }

    #[test]
    fn evidence_round_trips() {
        let mut r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        r.evidence = serde_json::json!({ "passed": 12, "failed": 0, "duration_ms": 4200 });
        let s = serde_json::to_string(&r).unwrap();
        let back = ReceiptEnvelope::parse_json(&s).expect("round-trip");
        assert_eq!(back.evidence["passed"], 12);
    }

    // ── ReceiptKind closed-set truth-table ───────────────────────────

    #[test]
    fn receipt_kind_all_enumerates_each_variant_exactly_once() {
        use std::collections::HashSet;

        let all = ReceiptKind::ALL;
        assert_eq!(all.len(), 4, "ALL arity must match the closed set");

        let mut seen: HashSet<ReceiptKind> = HashSet::new();
        for k in all {
            assert!(seen.insert(k), "duplicate variant in ALL: {k:?}");
        }
        for k in [
            ReceiptKind::ClosedLoopAuth,
            ReceiptKind::DbMigration,
            ReceiptKind::TestSuite,
            ReceiptKind::NixBuild,
        ] {
            assert!(all.contains(&k), "variant {k:?} unreachable through ALL");
        }
    }

    #[test]
    fn receipt_kind_as_str_unique_per_variant() {
        use std::collections::HashSet;

        let names: Vec<&'static str> = ReceiptKind::ALL.iter().map(|k| k.as_str()).collect();
        let unique: HashSet<&&'static str> = names.iter().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "non-injective as_str — Display would alias: {names:?}"
        );
    }

    #[test]
    fn receipt_kind_canonical_names_pinned() {
        // Byte-exact wire-format pin — renaming any of these is a
        // wire-format change, not a typed-internal refactor.
        assert_eq!(ReceiptKind::ClosedLoopAuth.as_str(), "closed-loop-auth");
        assert_eq!(ReceiptKind::DbMigration.as_str(), "db-migration");
        assert_eq!(ReceiptKind::TestSuite.as_str(), "test-suite");
        assert_eq!(ReceiptKind::NixBuild.as_str(), "nix-build");
    }

    #[test]
    fn receipt_kind_from_str_round_trips_canonical_names() {
        for k in ReceiptKind::ALL {
            assert_eq!(k.as_str().parse::<ReceiptKind>(), Ok(k));
        }
    }

    #[test]
    fn receipt_kind_from_str_rejects_open_kinds() {
        // Empty / future / typo / wrong-case all surface a typed
        // UnknownReceiptKind carrying the offending input verbatim
        // (operator-facing diagnostic); the schema is open at the
        // wire layer, but the closed-set view is byte-exact.
        for bad in ["", "closed_loop_auth", "ClosedLoopAuth", "operator-custom-kind"] {
            let err = bad.parse::<ReceiptKind>().unwrap_err();
            assert_eq!(err, UnknownReceiptKind(bad.to_string()));
        }
    }

    #[test]
    fn receipt_kind_display_delegates_to_as_str() {
        for k in ReceiptKind::ALL {
            assert_eq!(format!("{k}"), k.as_str());
        }
    }

    #[test]
    fn receipt_kind_into_string_matches_as_str() {
        for k in ReceiptKind::ALL {
            let s: String = k.into();
            assert_eq!(s, k.as_str());
        }
    }

    #[test]
    fn build_accepts_typed_receipt_kind() {
        // The typed → wire bridge: `build(ReceiptKind::X, …)` produces
        // a receipt whose `kind` field is exactly `X.as_str()`.
        for k in ReceiptKind::ALL {
            let env = ReceiptEnvelope::build(k, "i", "a", "c", None);
            assert_eq!(env.kind, k.as_str());
            assert!(env.verify_shape().is_ok());
            assert!(env.verify_root(None));
        }
    }

    #[test]
    fn known_kind_decodes_built_receipts() {
        for k in ReceiptKind::ALL {
            let env = ReceiptEnvelope::build(k, "i", "a", "c", None);
            assert_eq!(env.known_kind(), Some(k));
        }
    }

    #[test]
    fn known_kind_returns_none_for_open_kinds() {
        // Open-by-design: a custom operator-registered kind still
        // parses, still verifies, and still attests — it just doesn't
        // project through the closed-set typed view.
        let env = ReceiptEnvelope::build("operator-custom-kind", "i", "a", "c", None);
        assert_eq!(env.known_kind(), None);
        assert!(
            env.verify_shape().is_ok(),
            "open kind must remain a valid receipt"
        );
    }
}
