//! `tatara-receipt/v1` — the typed receipt envelope every pleme-io Job
//! emits to prove its work was done.
//!
//! Today's consumers (and the only ones supported on `tatara-receipt/v1`):
//! - **closed-loop auth probes** — `kind = "closed-loop-auth"`. Stamps
//!   that a system's bundled identity issuer authenticated its bundled
//!   client. The substrate primitive every closed-loop-testable product
//!   composes (an issuer↔client pair, future: identity providers,
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
//! shape the closed-loop-probe chart writes):
//!
//! ```yaml
//! version: tatara-receipt/v1
//! kind: closed-loop-auth
//! composed_root: <26-char hex>
//! intent_hash:   <hex>
//! artifact_hash: <hex>
//! control_hash:  <hex>
//! generated_at:  2026-05-19T22:00:00Z
//! process_ref:   "demo-test/ephemeral-demo"   # optional
//! evidence:      { ... }                              # optional, free-form
//! ```

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attestation::ProcessAttestation;

/// Canonical version string. Bump → `tatara-receipt/v2` if the wire
/// shape changes; parsers refuse anything else for the v1 reader.
pub const RECEIPT_VERSION: &str = "tatara-receipt/v1";

/// Suffix appended to a Job's name to compose its default receipt-
/// ConfigMap name. The substrate convention is that any Job which
/// emits a [`ReceiptEnvelope`] writes it to a ConfigMap in the Job's
/// own namespace whose name is `<job_name>-receipt` unless the caller
/// supplies an explicit override.
///
/// Load-bearing at three shipped derivation sites, each of which
/// re-composed the `<name>-receipt` shape by hand pre-lift:
/// - `tatara_reconciler::boundary::evaluate_job_attested` — the
///   `JobAttested` postcondition's default `receiptConfigMap`
///   (`<parsed.name>-receipt`);
/// - `tatara_reconciler::boundary::evaluate_closed_loop_auth` — the
///   `ClosedLoopAuth` postcondition's default `receiptConfigMap`
///   (`<probe_job_name>-receipt`, where the probe Job itself
///   defaults to `<process_name>-closed-loop-probe`);
/// - `tatara_reconciler::render::export_receipt_configmap_name` — the
///   export-worker Job's per-index receipt ConfigMap
///   (`<process_name>-export-<index>-receipt`), which is
///   structurally `<export_job_name(process_name, index)>-receipt`.
///
/// Pre-lift each site restated the suffix as a `format!` literal
/// (`format!("{}-receipt", parsed.name)`,
/// `format!("{job_name}-receipt")`, and
/// `format!("{process_name}-export-{index}-receipt")`). A rename to
/// `-attest` or a scheme change to `.receipt-cm` would have needed a
/// grep-and-replace across the three production sites AND a
/// coordinated update of every fleet-shipped operator override in the
/// closed-loop-probe chart and the reconciler's own tests.
/// Post-lift the suffix lives at ONE const on the receipt module;
/// [`default_receipt_config_map_name`] composes it with a Job name;
/// every default-derivation site AND the export-worker composer
/// route through the same primitive so a future suffix change lands
/// at this ONE const and every consumer picks it up mechanically.
///
/// Sibling suffix-const on the substrate: [`RECEIPT_VERSION`] pins
/// the wire-format version string every parser gates on; this const
/// pins the wire-K8s-name suffix every default-derivation site
/// composes. Both are load-bearing constants that operators grep and
/// dashboards template on — neither may drift from its published
/// spelling silently.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// `<job_name>-receipt` shape recurred at three production sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold and is lifted to
/// ONE substrate const + composer here. THEORY.md §III — the
/// typescape; the substrate's own receipt-CM naming convention
/// becomes a NAMED PRIMITIVE rather than a shape spelled out by hand
/// at every derivation site.
pub const RECEIPT_CM_SUFFIX: &str = "-receipt";

/// Compose the canonical receipt-ConfigMap name for a Job named
/// `job_name` — the substrate's default when no explicit
/// `receiptConfigMap` override is supplied on a postcondition's
/// params or when a renderer builds a Job whose receipt CM name is
/// derived from the Job's own name.
///
/// Returns `<job_name>{RECEIPT_CM_SUFFIX}`. See [`RECEIPT_CM_SUFFIX`]
/// for the full lift rationale and the three consumer sites the
/// primitive owns.
///
/// Uses string concatenation rather than `format!` so the composition
/// does not participate in the workspace's typed-emission ban migration
/// (skip-format-ban CLAUDE.md note); the shape is fixed at
/// `<job_name>` ++ [`RECEIPT_CM_SUFFIX`] and any future two-arg
/// composer (e.g. `default_receipt_config_map_name_scoped(cluster,
/// job)`) extends the primitive here, not the consumer sites.
#[must_use]
pub fn default_receipt_config_map_name(job_name: &str) -> String {
    let mut out = String::with_capacity(job_name.len() + RECEIPT_CM_SUFFIX.len());
    out.push_str(job_name);
    out.push_str(RECEIPT_CM_SUFFIX);
    out
}

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
#[closed_set(via = "as_str", display, generate_unknown)]
pub enum ReceiptKind {
    /// Closed-loop auth probe — stamps that a system's bundled identity
    /// issuer authenticated its bundled client. Emitted by
    /// `tatara-closed-loop-probe`; the substrate primitive every
    /// closed-loop-testable product composes (an issuer↔client pair,
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

// `impl fmt::Display for ReceiptKind` + `impl FromStr for ReceiptKind`
// + `impl tatara_lisp::ClosedSet for ReceiptKind` + `pub struct
// UnknownReceiptKind(pub String)` are generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(via =
// "as_str", display, generate_unknown)]` on the enum declaration above.
// The auto-derived label `"receipt kind"` matches the prior hand-
// rolled `#[error("unknown receipt kind: {0}")]` verbatim. The
// inherent `as_str` projection stays load-bearing — the kebab-case
// wire-format that matches `ReceiptEnvelope::kind`'s published literals
// verbatim — while the trait method `label` gives generic consumers a
// STABLE name across the workspace-wide closed-set implementors. The
// open-by-design `ReceiptEnvelope::known_kind` projection routes the
// `Err(UnknownReceiptKind)` arm into a `None` so operator-registered
// open kinds stay open.

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

/// One entry in the [`ReceiptEnvelope::REQUIRED_PILLARS`] closed-set
/// table — the pair (diagnostic field name, wire-form accessor) that
/// composes ONE required-pillar rejection through the shared
/// [`require_nonempty`] peer. The alias gives the tuple a nameable
/// type so downstream consumers (`tatara-check` receipt-inspector, an
/// LSP hover on the const, per-pillar dashboard columns) bind to
/// "one pillar's descriptor" as a first-class handle rather than
/// re-typing the underlying `(&'static str, fn(&ReceiptEnvelope) ->
/// &str)` tuple at every consumer.
pub type RequiredPillar = (&'static str, fn(&ReceiptEnvelope) -> &str);

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
            empty_to_none(&control_hash),
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

    /// Closed-set table of pillars that MUST be non-empty on every
    /// well-formed receipt — the wire-form's structural invariant
    /// [`Self::verify_shape`] enforces. Pre-lift the three checks
    /// lived as three byte-identical `if self.<pillar>.is_empty() {
    /// return Err(ReceiptError::MissingField("<pillar>")); }` two-arm
    /// conditionals inline in `verify_shape` — one per pillar name,
    /// each hand-writing the SAME (field-name, accessor, rejection)
    /// triple with the pillar name repeated at BOTH the accessor
    /// (`self.composed_root`) AND the diagnostic literal
    /// (`"composed_root"`). Post-lift the three (field-name,
    /// accessor) pairs live at ONE closed-set table here;
    /// `verify_shape` composes ONE per-entry iteration that
    /// dispatches through the shared [`require_nonempty`] free-fn
    /// peer of [`empty_to_none`].
    ///
    /// Each entry is a [`RequiredPillar`] tuple whose named type gives
    /// downstream consumers (a `tatara-check` receipt-inspector, an
    /// LSP hover, a per-pillar dashboard column) a nameable handle
    /// for "one pillar's (diagnostic-name, wire-form-accessor)
    /// pairing" rather than an unnamed function-pointer tuple
    /// re-typed at every consumer.
    ///
    /// The `control_hash` field is DELIBERATELY NOT in this table:
    /// the substrate's second pillar carries an "empty means absent"
    /// convention that [`Self::control_hash_opt`] + [`empty_to_none`]
    /// project as a typed `Option::None`, so its emptiness is a
    /// semantic bit rather than a validation failure. The pair
    /// (`REQUIRED_PILLARS` — must be non-empty; `control_hash_opt` —
    /// may be empty) is the substrate's typed answer to which
    /// pillars are load-bearing vs. schema-optional. A future
    /// re-shape that promotes a fourth required pillar (e.g. a
    /// mandatory `signer_hash` on a signed-receipt schema variant)
    /// lands as ONE new entry in this table + rustc's `[…; N]`
    /// arity constant on the type binding the extension in lockstep
    /// so a partial addition that forgets the diagnostic surface
    /// becomes a compile error rather than a runtime drift.
    ///
    /// Sibling closed-set tables across the crate:
    /// [`ReceiptKind::ALL`],
    /// [`crate::export::ReportFormat::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::intent::IntentKind::ALL`].
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the three inline pillar-emptiness checks recurred at THREE
    /// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
    /// and are lifted to ONE closed-set table + ONE shared rejection
    /// peer. THEORY.md §V.1 — knowable platform; the enumeration of
    /// required-pillar field names lives at ONE surface a
    /// documentation surface, an LSP hover, or a `tatara-check`
    /// receipt-inspector binds to for enumerating the receipt's
    /// structural invariants. THEORY.md §V.3 — three-pillar
    /// attestation; the (mandatory, may-be-absent) split of the
    /// three-pillar-plus-composed-root wire form is a typed
    /// substrate contract, not a per-consumer discipline.
    pub const REQUIRED_PILLARS: [RequiredPillar; 3] = [
        ("composed_root", |e| e.composed_root.as_str()),
        ("intent_hash", |e| e.intent_hash.as_str()),
        ("artifact_hash", |e| e.artifact_hash.as_str()),
    ];

    /// Verify the schema-level invariants: correct version + non-empty
    /// kind + non-empty pillar hashes (length-only, not BLAKE3-recompute).
    /// The three required-pillar rejections dispatch through
    /// [`Self::REQUIRED_PILLARS`] + [`require_nonempty`] so a future
    /// fourth required pillar lands at ONE table entry rather than
    /// as a fourth inline `if …is_empty() { return Err(…); }` copy.
    pub fn verify_shape(&self) -> Result<(), ReceiptError> {
        if self.version != RECEIPT_VERSION {
            return Err(ReceiptError::WrongVersion(self.version.clone()));
        }
        if self.kind.is_empty() {
            return Err(ReceiptError::EmptyKind);
        }
        for (field, accessor) in Self::REQUIRED_PILLARS {
            require_nonempty(field, accessor(self))?;
        }
        // control_hash MAY be empty when there is no control step;
        // the BLAKE3 compose treats empty as "absent" via Option —
        // see `Self::control_hash_opt` + `empty_to_none`.
        Ok(())
    }

    /// Verify that `composed_root` is consistent with the pillars.
    /// `expected_previous_root` is the previous root in the Process's
    /// attestation chain (or `None` for first attestation).
    pub fn verify_root(&self, expected_previous_root: Option<&str>) -> bool {
        let want = compose_root(
            &self.artifact_hash,
            self.control_hash_opt(),
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
            self.control_hash_opt().map(str::to_owned),
            self.intent_hash.clone(),
            previous_root.map(String::from),
            generation,
        )
    }

    /// Typed projection of the wire form's `control_hash` field —
    /// `Some(hash)` when a control step ran, `None` when it did not.
    ///
    /// The wire form stamps `control_hash: String` (schema-open,
    /// serde-friendly), but the substrate's `compose_root` +
    /// `ProcessAttestation::compose` compositions both take an
    /// `Option<&str>` / `Option<String>` and thread `None` through the
    /// exact BLAKE3 bytes pattern an absent-pillar walk emits — an
    /// empty `control_hash` and an absent-pillar receipt hash to the
    /// SAME `composed_root`. That "empty means absent" convention
    /// pre-lift lived at THREE sites inside this impl block —
    /// [`Self::build`] (constructing the envelope from typed pillars),
    /// [`Self::verify_root`] (recomposing the root against pillars for
    /// wire-form verification), and [`Self::to_attestation`] (lowering
    /// the receipt into a [`ProcessAttestation`] on a Process's
    /// attestation chain) — each hand-writing the SAME
    /// `if self.control_hash.is_empty() { None } else {
    /// Some(self.control_hash.as_str()) }` two-arm conditional. Post-
    /// lift the convention lives at ONE method here; the three
    /// consumers each compose a ONE-LINE call:
    ///   * `verify_root` → `self.control_hash_opt()` directly,
    ///   * `to_attestation` → `self.control_hash_opt().map(str::to_owned)`
    ///     for the `Option<String>` shape [`ProcessAttestation::compose`]
    ///     binds,
    ///   * `build` (which reads a local `control_hash: String` before
    ///     the envelope is constructed) → the free-fn peer
    ///     [`empty_to_none`] on the same borrowed string.
    ///
    /// Public because the projection is load-bearing operator-facing
    /// contract: an authoring surface (an LSP hover, a
    /// `tatara-check` report, a REPL `:receipt-inspect` command) that
    /// wants to render "no control step" vs. "control_hash: <hash>"
    /// binds to this method rather than pattern-matching on
    /// `self.control_hash.is_empty()` at its own call site — a future
    /// re-shape of the empty-means-absent convention (a sentinel-
    /// string variant, an explicit `Option<String>` on the wire form
    /// once the schema evolves, or a typed
    /// `ControlStep::{Ran(hash), Skipped}` enum) lands at ONE method
    /// here rather than at every consumer that inspects the pillar.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// wire-vs-typed projection lives at ONE substrate method so a
    /// consumer reads the pillar's typed-Option contract from the
    /// receipt directly, not from three parallel inline conditionals
    /// scattered across `build` / `verify_root` / `to_attestation`.
    /// THEORY.md §VI.1 — generation over composition; the
    /// `is_empty() ? None : Some(&self.control_hash)` two-arm
    /// projection recurred at THREE inline sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold and is lifted to ONE
    /// owner here. THEORY.md §V.3 — three-pillar attestation; the
    /// receipt's second pillar (control step) has ONE typed projection
    /// site the composition primitives ([`compose_root`],
    /// [`ProcessAttestation::compose`]) both bind against, so the
    /// pillar's wire-vs-typed identity cannot drift across the three
    /// consumers.
    #[must_use]
    pub fn control_hash_opt(&self) -> Option<&str> {
        empty_to_none(&self.control_hash)
    }
}

/// Project a wire-form pillar string onto its typed `Option<&str>`
/// contract — `Some(s)` when `s` is non-empty, `None` when `s` is
/// empty (the substrate's "no such pillar" convention that
/// [`compose_root`] + [`ProcessAttestation::compose`] both thread as
/// an absent-pillar walk through the BLAKE3 domain-tagged
/// composition).
///
/// The free-fn peer of [`ReceiptEnvelope::control_hash_opt`] for
/// call sites that hold a borrowed pillar string BEFORE a
/// [`ReceiptEnvelope`] is constructed — namely
/// [`ReceiptEnvelope::build`]'s inline `compose_root` call, which
/// composes the pillar's typed-Option identity from the local
/// `control_hash: String` intake before the envelope value exists.
/// The two peers share ONE projection body (`(!s.is_empty()).
/// then_some(s)`) so a future re-shape of the empty-means-absent
/// convention (a sentinel-string variant, an explicit
/// `Option<String>` on the wire form once the schema evolves)
/// lands at ONE substrate primitive rather than at both the
/// inherent method and its pre-construction free-fn peer.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition;
/// the pre-construction peer of the pillar projection lives at ONE
/// substrate primitive alongside the post-construction inherent
/// method, so the two receipt-lifecycle stages (pre-envelope in
/// [`ReceiptEnvelope::build`], post-envelope in every other
/// consumer) share ONE typed projection.
fn empty_to_none(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

/// Reject a required pillar whose wire form is empty with a typed
/// [`ReceiptError::MissingField`] carrying `field` — the diagnostic
/// literal the operator sees. Free-fn peer of [`empty_to_none`] on
/// the same wire-form-emptiness axis, and rejection sibling of the
/// [`ReceiptEnvelope::REQUIRED_PILLARS`] closed-set table
/// [`ReceiptEnvelope::verify_shape`] dispatches through.
///
/// The two peers on the emptiness axis carry two different typed
/// projections of the SAME wire-form bit:
///   * [`empty_to_none`] — the "empty means absent" convention for
///     the second pillar (control step); the substrate composes
///     `Option::None` through `compose_root` so an empty
///     `control_hash` and an absent-pillar receipt hash to the SAME
///     `composed_root`.
///   * [`require_nonempty`] — the "empty is a validation failure"
///     convention for the three required pillars; the substrate
///     rejects the receipt with a typed [`ReceiptError::MissingField`]
///     carrying the offending field name so the operator's diagnostic
///     surface (a reconciler event, a CLI stderr, a `tatara-check`
///     receipt-inspect report) names the pillar directly.
///
/// The two peers are DELIBERATELY named as (`empty_to_none`,
/// `require_nonempty`) rather than as a single overloaded projection
/// so the two typed conventions (absence-as-Option vs.
/// absence-as-Err) surface at the substrate's exported vocabulary
/// as two distinct primitives — a per-caller misroute (composing
/// `require_nonempty` on `control_hash` and getting a false
/// `MissingField`, or composing `empty_to_none` on `intent_hash`
/// and threading `None` through `compose_root` past a wire that
/// should have rejected) is a name-typo, not a silent semantic
/// swap.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition;
/// the "empty is a required-pillar failure" three-line inline
/// conditional recurred at THREE sites past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold and is lifted to ONE substrate primitive
/// composed through the [`ReceiptEnvelope::REQUIRED_PILLARS`] table.
/// THEORY.md §V.1 — knowable platform; the two emptiness projections
/// live at ONE typed vocabulary the receipt-inspection surfaces (LSP
/// hover, `tatara-check` report, REPL) bind to for reading the
/// receipt's structural contract from the substrate directly.
fn require_nonempty(field: &'static str, value: &str) -> Result<(), ReceiptError> {
    if value.is_empty() {
        return Err(ReceiptError::MissingField(field));
    }
    Ok(())
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
        r.process_ref = Some("demo-test/ephemeral".into());
        let s = serde_json::to_string(&r).unwrap();
        let back = ReceiptEnvelope::parse_json(&s).expect("round-trip");
        assert_eq!(back.process_ref.as_deref(), Some("demo-test/ephemeral"));
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

    /// Structural well-formedness of [`ReceiptKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `receipt_kind_all_enumerates_each_variant_exactly_once`
    /// + `receipt_kind_from_str_round_trips_canonical_names` + the
    /// empty-input arm of `receipt_kind_from_str_rejects_open_kinds`.
    /// `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path operators hit when parsing a wire
    /// `kind` field back to the typed kind.
    #[test]
    fn receipt_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<ReceiptKind>();
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
    fn receipt_kind_from_str_rejects_open_kinds() {
        // Future / typo / wrong-case all surface a typed
        // UnknownReceiptKind carrying the offending input verbatim
        // (operator-facing diagnostic); the schema is open at the
        // wire layer, but the closed-set view is byte-exact. The
        // empty-input arm is pinned by
        // [`receipt_kind_is_well_formed_closed_set`] via the
        // `tatara_lisp::ClosedSet` testkit; the cases here pin the
        // verbatim-echo contract on the [`UnknownReceiptKind`] newtype,
        // which the trait's `make_unknown` can't see.
        for bad in ["closed_loop_auth", "ClosedLoopAuth", "operator-custom-kind"] {
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

    // ── `control_hash_opt` / `empty_to_none` — the wire-form-to-typed
    //    projection of the second pillar (control step). Pre-lift the
    //    `is_empty() ? None : Some(&self.control_hash)` two-arm
    //    conditional lived at THREE inline sites — `build`,
    //    `verify_root`, `to_attestation` — each hand-writing the SAME
    //    projection with slightly-different ownership shapes
    //    (`Option<&str>` for the two composers, `Option<String>` for
    //    the attestation composer). Post-lift the projection lives at
    //    ONE inherent method + ONE free-fn peer for the pre-envelope
    //    call site. The tests below pin the substrate primitive's
    //    contract at its boundary so a regression at the projection
    //    surfaces here rather than as a silent `composed_root` drift
    //    at every consumer that composes the pillar.

    #[test]
    fn empty_to_none_projects_empty_to_none_and_non_empty_to_some_verbatim() {
        // The pre-envelope free-fn peer of `control_hash_opt` — used
        // by `build` before the envelope exists. Pin BOTH arms of the
        // projection: an empty string projects to `None` (the "no
        // such pillar" convention that `compose_root` threads through
        // the absent-pillar BLAKE3 bytes pattern), and any non-empty
        // string projects to `Some(s)` byte-identical to the input.
        // A regression that (a) inverted the arms (folding `""` to
        // `Some("")` and every non-empty into `None`), (b) normalized
        // the payload (trimming whitespace, lowercasing hex), or (c)
        // introduced a sentinel-string special case (`"none"`, `"-"`,
        // etc.) would surface here rather than as a silent
        // `composed_root` shift at every consumer that composes the
        // pillar.
        assert_eq!(super::empty_to_none(""), None);
        assert_eq!(super::empty_to_none("c"), Some("c"));
        assert_eq!(super::empty_to_none("cccc"), Some("cccc"));
        // A whitespace-only string is NOT empty by the pillar's typed
        // contract — the substrate composes bytes verbatim through
        // BLAKE3, so a `" "` control hash IS a distinct pillar from
        // an absent one; the projection must preserve that
        // distinction.
        assert_eq!(super::empty_to_none(" "), Some(" "));
    }

    #[test]
    fn control_hash_opt_matches_the_free_fn_peer_on_every_receipt() {
        // Post-envelope inherent method routes through the same
        // `empty_to_none` free-fn body — pin the equivalence across
        // both arms so a future regression that split the two
        // projections (e.g. the inherent method starts trimming, the
        // free-fn stays byte-verbatim) surfaces here rather than as a
        // `composed_root` mismatch between `build` (uses the free-fn
        // peer) and `verify_root` / `to_attestation` (use the
        // inherent method).
        let with_control = ReceiptEnvelope::build("test-suite", "i", "a", "cccc", None);
        assert_eq!(with_control.control_hash_opt(), Some("cccc"));
        assert_eq!(
            with_control.control_hash_opt(),
            super::empty_to_none(&with_control.control_hash),
        );

        let no_control = ReceiptEnvelope::build("nix-build", "i", "a", "", None);
        assert_eq!(no_control.control_hash_opt(), None);
        assert_eq!(
            no_control.control_hash_opt(),
            super::empty_to_none(&no_control.control_hash),
        );
    }

    #[test]
    fn control_hash_opt_composes_the_same_root_the_three_consumers_bind() {
        // End-to-end pin at the receipt-lifecycle boundary — the
        // three consumers (`build`, `verify_root`, `to_attestation`)
        // must land on the SAME `composed_root` for a given pillar
        // tuple regardless of which projection body they route
        // through. Sweeps BOTH pillar arms (present control + empty
        // control) so a regression that mis-wired ONE consumer to
        // the pre-lift inline conditional or that changed the
        // projection at ONE site surfaces here rather than as a
        // silent divergence between `verify_root`'s decision and
        // `to_attestation`'s written `composed_root`.
        for control in ["", "control-hash-cccc"] {
            let env = ReceiptEnvelope::build("test-suite", "i", "a", control, None);
            // `verify_root` composes through the inherent method AND
            // through the same `compose_root(&artifact, control_opt,
            // &intent, previous)` skeleton `build` binds — so the
            // envelope must verify against its own composed root.
            assert!(
                env.verify_root(None),
                "verify_root failed for control={control:?}",
            );
            // `to_attestation` composes the same pillar tuple through
            // `ProcessAttestation::compose`'s `Option<String>` shape;
            // the attestation's `composed_root` must match the
            // envelope's `composed_root` byte-for-byte because both
            // compose the SAME BLAKE3 domain-tagged skeleton over
            // the SAME typed-Option pillar identity.
            let att = env.to_attestation(0, None);
            assert_eq!(
                att.composed_root, env.composed_root,
                "attestation root drift for control={control:?}",
            );
        }
    }

    // ── `REQUIRED_PILLARS` / `require_nonempty` — the closed-set
    //    table + shared rejection peer that `verify_shape` composes
    //    the three required-pillar emptiness checks through. Pre-lift
    //    the three `if self.<pillar>.is_empty() { return Err(
    //    ReceiptError::MissingField("<pillar>")); }` two-arm
    //    conditionals lived inline in `verify_shape` — one per pillar
    //    name, each hand-writing the SAME (field-name, accessor,
    //    rejection) triple. Post-lift the three (field-name,
    //    accessor) pairs live at ONE `REQUIRED_PILLARS` const, and
    //    the rejection body lives at ONE `require_nonempty` peer. The
    //    tests below pin the substrate primitives' contract at their
    //    boundary so a regression at the projection surfaces here
    //    rather than as a silent shift in the receipt's structural
    //    validation semantics.

    #[test]
    fn require_nonempty_rejects_empty_with_the_named_field_and_passes_non_empty_verbatim() {
        // The shared rejection peer of `empty_to_none` — used by
        // `verify_shape` through the `REQUIRED_PILLARS` sweep. Pin
        // BOTH arms of the projection: an empty value rejects with
        // `ReceiptError::MissingField(field)` carrying the literal
        // `field` byte-identically (so a rename at the table entry
        // reaches the operator's diagnostic surface — the reconciler
        // event, the CLI stderr, the `tatara-check` receipt-inspect
        // report), and any non-empty value passes with `Ok(())`
        // (regardless of the payload's shape — a whitespace-only `" "`
        // is NOT empty by the pillar's typed contract). A regression
        // that (a) mis-named the field on the rejection (leaking a
        // caller-controlled `&str` in place of the `&'static` diagnostic
        // literal), (b) rejected non-empty values (folding `" "` or
        // some other sentinel into `MissingField`), or (c) accepted
        // the empty payload silently would surface here rather than
        // as a silent semantic shift in `verify_shape`'s rejection
        // vocabulary.
        assert_eq!(
            super::require_nonempty("composed_root", ""),
            Err(ReceiptError::MissingField("composed_root")),
        );
        assert_eq!(
            super::require_nonempty("intent_hash", ""),
            Err(ReceiptError::MissingField("intent_hash")),
        );
        assert_eq!(super::require_nonempty("composed_root", "aaaa"), Ok(()));
        // Whitespace-only strings pass the rejection gate — the
        // substrate composes bytes verbatim through BLAKE3 so `" "`
        // IS a distinct pillar from an absent one; the rejection
        // must preserve that distinction.
        assert_eq!(super::require_nonempty("intent_hash", " "), Ok(()));
    }

    #[test]
    fn required_pillars_table_is_pairwise_distinct_and_enumerates_the_three_names() {
        // The closed-set table `verify_shape` dispatches through.
        // Pin: (a) the arity is exactly THREE (rustc's `[…; 3]`
        // constant on the type binds this at compile time; the pin
        // here checks the runtime enumeration matches so a future
        // arity bump surfaces as a coordinated update rather than a
        // silent drift), (b) each entry's field name is a
        // pillar-unique string (a duplicate entry — the same pillar
        // listed twice — would evaluate the same rejection twice at
        // ONE run, hiding a distinct pillar's absence behind the
        // duplicate's success), (c) the three names match the
        // byte-exact wire literals the reconciler tests + operator
        // diagnostics have already published (`"composed_root"`,
        // `"intent_hash"`, `"artifact_hash"`) — renaming any of them
        // is a wire-diagnostic change, not a typed-internal refactor.
        let names: Vec<&'static str> = ReceiptEnvelope::REQUIRED_PILLARS
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names, vec!["composed_root", "intent_hash", "artifact_hash"]);

        // Pairwise-distinct check — the table's arity is small
        // enough for a hand-authored O(n^2) sweep, and a duplicate
        // would defeat the whole point of the enumeration.
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                assert_ne!(
                    names[i], names[j],
                    "REQUIRED_PILLARS[{i}] and [{j}] share field name {}",
                    names[i],
                );
            }
        }

        // control_hash is DELIBERATELY not in the table (it carries
        // the "empty means absent" semantic bit — see
        // `control_hash_opt` + `empty_to_none`). Pin the exclusion so
        // a future well-meaning addition that promotes control_hash
        // to a required pillar surfaces here as a contract change
        // rather than as a silent rejection of receipts the
        // substrate's own compose_root treats as valid absent-pillar
        // walks.
        assert!(
            !names.contains(&"control_hash"),
            "control_hash must not be in REQUIRED_PILLARS — its emptiness \
             is the substrate's absent-pillar convention",
        );
    }

    #[test]
    fn verify_shape_rejects_each_required_pillar_when_emptied_with_the_typed_field_name() {
        // End-to-end pin at the `verify_shape` boundary — each entry
        // in `REQUIRED_PILLARS` must surface a
        // `ReceiptError::MissingField(field)` carrying the entry's
        // OWN name when its accessor's value is empty. Sweeps the
        // table so a future fourth required pillar picks up the
        // rejection through the SAME per-entry iteration + the SAME
        // shared `require_nonempty` peer, and a mis-wired accessor
        // (an entry naming "intent_hash" whose accessor reads
        // `self.artifact_hash`) surfaces here as a mismatched typed
        // rejection rather than as a silent semantic drift at
        // production.
        for (field, accessor) in ReceiptEnvelope::REQUIRED_PILLARS {
            let mut env = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
            // Empty ONLY the pillar under test by zeroing the field
            // through the wire-form struct's own mutable access
            // (which the `#[serde(deny_unknown_fields)]` wire shape
            // doesn't restrict at the Rust level).
            match field {
                "composed_root" => env.composed_root.clear(),
                "intent_hash" => env.intent_hash.clear(),
                "artifact_hash" => env.artifact_hash.clear(),
                other => panic!("unknown REQUIRED_PILLARS entry {other}"),
            }
            assert!(
                accessor(&env).is_empty(),
                "accessor for {field} did not read the emptied field",
            );
            let err = env
                .verify_shape()
                .expect_err("verify_shape must reject empty required pillar");
            assert_eq!(
                err,
                ReceiptError::MissingField(field),
                "verify_shape returned {err:?} — expected MissingField({field:?})",
            );
        }
    }

    // ── RECEIPT_CM_SUFFIX + default_receipt_config_map_name ──────────
    //
    // Fail-before-pass-after pins for the substrate-level naming
    // convention that the reconciler's JobAttested + ClosedLoopAuth
    // evaluators AND the export-worker renderer all route through
    // for their default-derivation callsites. A regression that
    // renamed the suffix (e.g. `-receipt` → `-attest`, `-cm`, or
    // `.receipt`) OR that swapped the composition order at the
    // composer (e.g. `<suffix><job>` instead of `<job><suffix>`)
    // would silently misroute every default-derivation receipt-CM
    // read against a ConfigMap the Job never wrote to — the pins
    // here catch the drift at the primitive itself, before it
    // reaches any downstream consumer.

    #[test]
    fn receipt_cm_suffix_pinned_to_dash_receipt() {
        // Byte-exact wire-format pin — renaming this is a wire-name
        // change, not a typed-internal refactor. Operators grep for
        // the `-receipt` suffix in kubectl output; dashboards and
        // export tooling template on it; the closed-loop-probe chart
        // publishes ConfigMaps at this suffix. A silent rename here
        // would desync all of them at once.
        assert_eq!(RECEIPT_CM_SUFFIX, "-receipt");
    }

    #[test]
    fn default_receipt_config_map_name_appends_suffix_to_job_name() {
        // The canonical composition every default-derivation site
        // routed through pre-lift as `format!("{name}-receipt")`.
        assert_eq!(default_receipt_config_map_name("my-job"), "my-job-receipt");
        assert_eq!(
            default_receipt_config_map_name("probe-job"),
            "probe-job-receipt"
        );
    }

    #[test]
    fn default_receipt_config_map_name_composes_through_the_suffix_const() {
        // Cross-primitive coherence pin — the composer's output must
        // equal `<job_name>{RECEIPT_CM_SUFFIX}` verbatim across a
        // sweep of shipped Job-name shapes (bare, hierarchical
        // export-index, closed-loop probe derivation, one-char, and
        // empty). A regression that inlined the suffix at the
        // composer (breaking the const's role as the ONE source of
        // truth) fails HERE at the shipped-shape sweep because the
        // pin re-reads the const at test time.
        for job_name in [
            "my-job",
            "r1-export-0",
            "attest-export-5",
            "closed-loop-attest-closed-loop-probe",
            "x",
            "",
        ] {
            let mut expected = String::new();
            expected.push_str(job_name);
            expected.push_str(RECEIPT_CM_SUFFIX);
            assert_eq!(
                default_receipt_config_map_name(job_name),
                expected,
                "default_receipt_config_map_name({job_name:?}) drifted from \
                 <job>++RECEIPT_CM_SUFFIX composition",
            );
        }
    }

    #[test]
    fn default_receipt_config_map_name_matches_prior_hand_authored_format_shape() {
        // Path-uniformity pin against the three pre-lift `format!`
        // literals — each callsite spelled the shape a slightly
        // different way (`format!("{}-receipt", parsed.name)` /
        // `format!("{job_name}-receipt")` /
        // `format!("{process_name}-export-{index}-receipt")`) but
        // all three composed the SAME `<job>-receipt` byte sequence
        // once evaluated. The lift preserves that byte identity so
        // no downstream ConfigMap grep or fleet-shipped operator
        // override changes meaning. A regression at the primitive
        // that broke the byte identity (e.g. inserted a separator,
        // uppercased the suffix, dropped the leading dash) would
        // fail HERE against the pre-lift `format!` literal for a
        // hand-picked Job-name that carries no ambiguity around
        // separators.
        let job_name = "svc-abc-export-3";
        let pre_lift = format!("{job_name}-receipt");
        let post_lift = default_receipt_config_map_name(job_name);
        assert_eq!(
            pre_lift, post_lift,
            "post-lift primitive drifted from pre-lift `format!(\"{{name}}-receipt\")` byte shape",
        );
    }
}
