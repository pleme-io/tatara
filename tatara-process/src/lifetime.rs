//! Process lifetime — Permanent (re-converging) vs Ephemeral (auto-SIGTERM
//! on Attested / TTL / Failed).
//!
//! The wire shape follows the same "exactly-one-optional-field" pattern as
//! `Intent` — one tagged-union idiom across the typescape.
//!
//! Lisp authoring:
//! ```lisp
//! :lifetime (:permanent)
//! :lifetime (:ephemeral :ttl "1h"
//!                       :teardown OnAttested
//!                       :max-concurrent 1)
//! ```
//!
//! Default = `Permanent` — every existing Process keeps its current behavior.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::export::ExportSpec;
use crate::phase::ProcessPhase;

/// Lifetime slot on `ProcessSpec`. Exactly one variant should be populated;
/// when both are unset the resolver returns `Permanent`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Lifetime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permanent: Option<PermanentLifetime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<EphemeralLifetime>,
}

/// Resolved enum view used by the reconciler.
#[derive(Clone, Debug)]
pub enum LifetimeVariant<'a> {
    Permanent(&'a PermanentLifetime),
    Ephemeral(&'a EphemeralLifetime),
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LifetimeError {
    #[error("lifetime has multiple variants set; at most one required")]
    Ambiguous,
}

impl Lifetime {
    /// True when no variant is set — treated as `Permanent` by the resolver.
    pub fn is_default(&self) -> bool {
        self.permanent.is_none() && self.ephemeral.is_none()
    }

    /// Resolve to a variant view. Empty resolves to `Permanent` (a static
    /// borrow on the embedded `DEFAULT_PERMANENT`); ambiguous (both set) is
    /// an error.
    pub fn variant(&self) -> Result<LifetimeVariant<'_>, LifetimeError> {
        use crate::tagged_union::{resolve, ResolveError};
        match resolve([
            self.permanent.as_ref().map(LifetimeVariant::Permanent),
            self.ephemeral.as_ref().map(LifetimeVariant::Ephemeral),
        ]) {
            Ok(v) => Ok(v),
            Err(ResolveError::None) => Ok(LifetimeVariant::Permanent(&DEFAULT_PERMANENT)),
            Err(ResolveError::Many) => Err(LifetimeError::Ambiguous),
        }
    }

    /// True iff `ephemeral` is set.
    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral.is_some()
    }
}

const DEFAULT_PERMANENT: PermanentLifetime = PermanentLifetime {};

/// Permanent lifetime — the existing Process behavior. SIGHUP re-converges;
/// SIGTERM terminates only on explicit operator action.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermanentLifetime {}

/// Ephemeral lifetime — Process auto-terminates per `teardown_policy`.
///
/// Phase semantics:
/// - On `Attested` with `teardown_policy ∈ {OnAttested, Always}`:
///   reconciler delivers SIGTERM, Process drives Exiting → Zombie → Reaped.
/// - On `Failed`  with `teardown_policy ∈ {OnFailed,   Always}`:
///   same. Otherwise Process stays at Failed for forensic inspection.
/// - `ttl` is a `humantime` duration (`"1h"`, `"30m"`) checked at every
///   reconcile loop tick. TTL expiry while in any non-terminal phase
///   forces SIGTERM regardless of `teardown_policy`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EphemeralLifetime {
    /// `humantime`-parseable duration from `phaseSince(Forking)` after
    /// which the Process is force-SIGTERM'd.
    #[serde(default = "default_ttl")]
    pub ttl: String,

    /// When the Process auto-terminates.
    #[serde(default)]
    pub teardown_policy: TeardownPolicy,

    /// Cluster-wide concurrency budget across ephemeral Processes that
    /// share the same `spec.identity.name_override` / chart_ref.
    /// `0` = no cap. Enforced by the reconciler before transitioning out
    /// of `Pending`.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,

    /// Declared exports — what artifacts survive teardown and where
    /// they flow. Empty (default) = nothing survives, matching the
    /// "ephemeral leaves no trace" posture. Each `ExportSpec` is
    /// independently triggered during the reconciler's `Releasing`
    /// phase against the terminal `ProcessPhase` reached.
    ///
    /// See [`crate::export`] for the full type. All exports flow
    /// through the pleme-io Vector + NATS layer — there is no
    /// per-spec ad-hoc sink.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<ExportSpec>,
}

impl EphemeralLifetime {
    /// True iff any declared export's [`crate::export::ExportTrigger`]
    /// fires for the given terminal-reached phase. The reconciler
    /// uses this to decide whether to route `Attested`/`Failed`
    /// through `Releasing` (the export window) or skip straight to
    /// `Exiting`/`Zombie`.
    ///
    /// Returns `false` when the export list is empty or no trigger
    /// matches — both cases collapse to the existing teardown path.
    pub fn has_applicable_exports(&self, phase: ProcessPhase) -> bool {
        self.exports.iter().any(|e| match phase {
            ProcessPhase::Attested => e.when.fires_on_attested(),
            ProcessPhase::Failed => e.when.fires_on_failed(),
            _ => false,
        })
    }

    /// Iterate over the exports whose trigger fires on `phase`.
    /// The reconciler's `handle_releasing` consumes this to emit
    /// one tatara-export-worker Job per surviving spec.
    pub fn applicable_exports(
        &self,
        phase: ProcessPhase,
    ) -> impl Iterator<Item = &ExportSpec> + '_ {
        self.exports.iter().filter(move |e| match phase {
            ProcessPhase::Attested => e.when.fires_on_attested(),
            ProcessPhase::Failed => e.when.fires_on_failed(),
            _ => false,
        })
    }
}

impl Default for EphemeralLifetime {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            teardown_policy: TeardownPolicy::default(),
            max_concurrent: default_max_concurrent(),
            exports: Vec::new(),
        }
    }
}

fn default_ttl() -> String {
    "1h".to_string()
}
fn default_max_concurrent() -> u32 {
    1
}

/// When an ephemeral Process self-terminates.
///
/// Aligns with `ProcessPhase` (`Attested` / `Failed`) rather than borrowing
/// foreign success/failure language — typed phases are the source of truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum TeardownPolicy {
    /// SIGTERM as soon as the Process reaches `Attested` or `Failed`.
    #[default]
    Always,
    /// SIGTERM only on `Attested`. Leave `Failed` Processes for inspection.
    OnAttested,
    /// SIGTERM only on `Failed`. Leave `Attested` Processes running until
    /// TTL or explicit operator SIGTERM.
    OnFailed,
    /// Never auto-terminate (TTL still applies).
    Never,
}

impl TeardownPolicy {
    /// True iff, given a terminal phase, this policy says "tear down."
    pub fn should_teardown_on_attested(self) -> bool {
        matches!(self, Self::Always | Self::OnAttested)
    }
    pub fn should_teardown_on_failed(self) -> bool {
        matches!(self, Self::Always | Self::OnFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lifetime_resolves_to_permanent() {
        let l = Lifetime::default();
        assert!(l.is_default());
        assert!(!l.is_ephemeral());
        assert!(matches!(
            l.variant().unwrap(),
            LifetimeVariant::Permanent(_)
        ));
    }

    #[test]
    fn ephemeral_set_resolves() {
        let l = Lifetime {
            ephemeral: Some(EphemeralLifetime::default()),
            ..Lifetime::default()
        };
        assert!(l.is_ephemeral());
        match l.variant().unwrap() {
            LifetimeVariant::Ephemeral(e) => {
                assert_eq!(e.ttl, "1h");
                assert_eq!(e.teardown_policy, TeardownPolicy::Always);
                assert_eq!(e.max_concurrent, 1);
            }
            other => panic!("expected ephemeral, got {other:?}"),
        }
    }

    #[test]
    fn ambiguous_lifetime_errors() {
        let l = Lifetime {
            permanent: Some(PermanentLifetime {}),
            ephemeral: Some(EphemeralLifetime::default()),
        };
        assert_eq!(l.variant().unwrap_err(), LifetimeError::Ambiguous);
    }

    #[test]
    fn teardown_policy_dispatch() {
        assert!(TeardownPolicy::Always.should_teardown_on_attested());
        assert!(TeardownPolicy::Always.should_teardown_on_failed());
        assert!(TeardownPolicy::OnAttested.should_teardown_on_attested());
        assert!(!TeardownPolicy::OnAttested.should_teardown_on_failed());
        assert!(!TeardownPolicy::OnFailed.should_teardown_on_attested());
        assert!(TeardownPolicy::OnFailed.should_teardown_on_failed());
        assert!(!TeardownPolicy::Never.should_teardown_on_attested());
        assert!(!TeardownPolicy::Never.should_teardown_on_failed());
    }

    #[test]
    fn serde_round_trip_ephemeral() {
        let l = Lifetime {
            ephemeral: Some(EphemeralLifetime {
                ttl: "30m".into(),
                teardown_policy: TeardownPolicy::OnAttested,
                max_concurrent: 4,
                exports: vec![],
            }),
            ..Lifetime::default()
        };
        let yaml = serde_yaml::to_string(&l).unwrap();
        assert!(yaml.contains("ttl: 30m"));
        assert!(yaml.contains("teardownPolicy: OnAttested"));
        // Empty exports skip-serialize — explicit zero-trace default.
        assert!(!yaml.contains("exports"));
        let back: Lifetime = serde_yaml::from_str(&yaml).unwrap();
        assert!(back.is_ephemeral());
        assert!(back.ephemeral.unwrap().exports.is_empty());
    }

    #[test]
    fn applicable_exports_filters_by_trigger() {
        use crate::export::{
            ArtifactSource, ExportSpec, ExportTrigger, HttpEventChannel, ReceiptsSource,
            VectorChannel,
        };
        let spec_attested = ExportSpec {
            source: ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: "receipt".into(),
                }),
                ..VectorChannel::default()
            },
            when: ExportTrigger::OnAttested,
            experiment_id_override: None,
        };
        let spec_failed = ExportSpec {
            when: ExportTrigger::OnFailed,
            ..spec_attested.clone()
        };
        let spec_always = ExportSpec {
            when: ExportTrigger::Always,
            ..spec_attested.clone()
        };

        let lt = EphemeralLifetime {
            ttl: "1h".into(),
            teardown_policy: TeardownPolicy::OnAttested,
            max_concurrent: 1,
            exports: vec![spec_attested, spec_failed, spec_always],
        };

        // Attested gate fires OnAttested + Always — 2 of 3.
        assert!(lt.has_applicable_exports(ProcessPhase::Attested));
        assert_eq!(lt.applicable_exports(ProcessPhase::Attested).count(), 2);

        // Failed gate fires OnFailed + Always — 2 of 3.
        assert!(lt.has_applicable_exports(ProcessPhase::Failed));
        assert_eq!(lt.applicable_exports(ProcessPhase::Failed).count(), 2);

        // Other phases never route through Releasing.
        for p in [
            ProcessPhase::Pending,
            ProcessPhase::Forking,
            ProcessPhase::Execing,
            ProcessPhase::Running,
            ProcessPhase::Reconverging,
            ProcessPhase::Releasing,
            ProcessPhase::Exiting,
            ProcessPhase::Zombie,
            ProcessPhase::Reaped,
        ] {
            assert!(!lt.has_applicable_exports(p));
            assert_eq!(lt.applicable_exports(p).count(), 0);
        }
    }

    #[test]
    fn no_exports_means_no_applicable_exports() {
        let lt = EphemeralLifetime::default();
        assert!(!lt.has_applicable_exports(ProcessPhase::Attested));
        assert!(!lt.has_applicable_exports(ProcessPhase::Failed));
    }

    #[test]
    fn exports_round_trip_through_lifetime() {
        use crate::export::{
            ArtifactSource, ExportSpec, ExportTrigger, HttpEventChannel, ReceiptsSource,
            VectorChannel,
        };
        let l = Lifetime {
            ephemeral: Some(EphemeralLifetime {
                ttl: "30m".into(),
                teardown_policy: TeardownPolicy::OnAttested,
                max_concurrent: 1,
                exports: vec![ExportSpec {
                    source: ArtifactSource {
                        receipts: Some(ReceiptsSource::default()),
                        ..ArtifactSource::default()
                    },
                    channel: VectorChannel {
                        http_event: Some(HttpEventChannel {
                            endpoint: None,
                            signal_type: "receipt".into(),
                        }),
                        ..VectorChannel::default()
                    },
                    when: ExportTrigger::OnAttested,
                    experiment_id_override: None,
                }],
            }),
            ..Lifetime::default()
        };
        let yaml = serde_yaml::to_string(&l).unwrap();
        assert!(yaml.contains("exports:"));
        assert!(yaml.contains("receipts: {}"));
        assert!(yaml.contains("signalType: receipt"));
        let back: Lifetime = serde_yaml::from_str(&yaml).unwrap();
        let e = back.ephemeral.unwrap();
        assert_eq!(e.exports.len(), 1);
        assert!(e.exports[0].source.receipts.is_some());
        assert!(e.exports[0].channel.http_event.is_some());
    }
}
