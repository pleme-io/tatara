//! Overlay composition + application — nixpkgs's `self: super: …` pattern, typed.
//!
//! A `PackageSet` is a dictionary of derivations keyed by name. An overlay
//! extends it with new packages (`adds`) and replaces existing ones
//! (`replaces`). Composing N overlays produces one equivalent overlay;
//! applying that overlay to a base package set yields the fully-overlaid result.
//!
//! Semantics match Nix: overlays are applied in sequence, each sees the output
//! of the previous. Later overlays override earlier ones when they touch the
//! same package name.

use std::collections::BTreeMap;

use crate::derivation::Derivation;
use crate::overlay::{Overlay, OverlayTarget, Replacement};

/// The "base" — a dictionary of packages keyed by `Derivation::name`.
pub type PackageSet = BTreeMap<String, Derivation>;

/// Apply a single overlay to a package set. Returns a new (cloned) set.
///   - `overlay.replaces` substitutes existing packages by `upstream_name`
///   - `overlay.adds` inserts new packages; if a name collides with an existing
///     entry the add wins (matches `self: super: { foo = …; }` semantics
///     where the left-hand side prevails)
pub fn apply(overlay: &Overlay, base: &PackageSet) -> PackageSet {
    let mut out = base.clone();
    for Replacement { upstream_name, with } in &overlay.replaces {
        out.insert(upstream_name.clone(), with.clone());
    }
    for add in &overlay.adds {
        out.insert(add.name.clone(), add.clone());
    }
    out
}

/// Apply a chain of overlays in order.
pub fn apply_chain(overlays: &[Overlay], base: &PackageSet) -> PackageSet {
    let mut out = base.clone();
    for o in overlays {
        out = apply(o, &out);
    }
    out
}

/// Compose N overlays into a single equivalent overlay.
/// The composition, applied to any base, matches applying the chain in order.
///
/// Semantics:
///   - `target`: taken from the first overlay; heterogeneous composition
///     (differing targets) is an explicit error — you can't meaningfully merge
///     a `PackageSet` overlay with a `Module` overlay.
///   - `adds`: union across all overlays; later-added same-name wins.
///   - `replaces`: replacements collected in order; later `replaces` override
///     earlier `replaces` on the same `upstream_name`.
pub fn compose(overlays: &[Overlay]) -> Result<Overlay, ComposeError> {
    if overlays.is_empty() {
        return Ok(Overlay {
            name: "composed-empty".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![],
            replaces: vec![],
            description: Some("empty composition".into()),
        });
    }
    let target = overlays[0].target;
    for o in overlays {
        if o.target != target {
            return Err(ComposeError::TargetMismatch {
                first: target,
                other: o.target,
                overlay: o.name.clone(),
            });
        }
    }

    // Track adds + replaces by name so last-wins within the composition.
    let mut adds: BTreeMap<String, Derivation> = BTreeMap::new();
    let mut replaces: BTreeMap<String, Derivation> = BTreeMap::new();

    for o in overlays {
        for add in &o.adds {
            adds.insert(add.name.clone(), add.clone());
        }
        for Replacement { upstream_name, with } in &o.replaces {
            replaces.insert(upstream_name.clone(), with.clone());
        }
    }

    let name = overlays
        .iter()
        .map(|o| o.name.as_str())
        .collect::<Vec<_>>()
        .join("+");

    Ok(Overlay {
        name: format!("composed-{name}"),
        target,
        adds: adds.into_values().collect(),
        replaces: replaces
            .into_iter()
            .map(|(upstream_name, with)| Replacement { upstream_name, with })
            .collect(),
        description: Some(format!("composition of {} overlays", overlays.len())),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    #[error("cannot compose overlays with different targets: {first:?} vs {other:?} (in {overlay:?})")]
    TargetMismatch {
        first: OverlayTarget,
        other: OverlayTarget,
        overlay: String,
    },
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn drv(name: &str, version: Option<&str>) -> Derivation {
        Derivation {
            name: name.into(),
            version: version.map(String::from),
            inputs: vec![],
            source: Default::default(),
            builder: Default::default(),
            outputs: Default::default(),
            env: vec![],
            sandbox: Default::default(),
            bridge: None,
        }
    }

    fn base() -> PackageSet {
        let mut set = PackageSet::new();
        set.insert("hello".into(), drv("hello", Some("2.12.1")));
        set.insert("glibc".into(), drv("glibc", Some("2.38")));
        set
    }

    #[test]
    fn apply_adds_inserts_new_packages() {
        let o = Overlay {
            name: "add".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![drv("curl", Some("8.0"))],
            replaces: vec![],
            description: None,
        };
        let out = apply(&o, &base());
        assert_eq!(out.len(), 3);
        assert_eq!(out["curl"].version.as_deref(), Some("8.0"));
    }

    #[test]
    fn apply_replaces_substitutes_by_name() {
        let o = Overlay {
            name: "patch-hello".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![],
            replaces: vec![Replacement {
                upstream_name: "hello".into(),
                with: drv("hello-patched", Some("2.12.1-p1")),
            }],
            description: None,
        };
        let out = apply(&o, &base());
        // Key stays "hello" (upstream name); derivation replaced.
        assert_eq!(out.len(), 2);
        assert_eq!(out["hello"].name, "hello-patched");
        assert_eq!(out["hello"].version.as_deref(), Some("2.12.1-p1"));
    }

    #[test]
    fn apply_chain_applies_in_order() {
        let a = Overlay {
            name: "a".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![drv("curl", Some("8.0"))],
            replaces: vec![],
            description: None,
        };
        let b = Overlay {
            name: "b".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![drv("curl", Some("8.5"))], // later overrides earlier
            replaces: vec![],
            description: None,
        };
        let out = apply_chain(&[a, b], &base());
        assert_eq!(out["curl"].version.as_deref(), Some("8.5"));
    }

    #[test]
    fn compose_empty_returns_empty_overlay() {
        let composed = compose(&[]).unwrap();
        assert!(composed.adds.is_empty());
        assert!(composed.replaces.is_empty());
    }

    #[test]
    fn compose_two_overlays_matches_chain_application() {
        let a = Overlay {
            name: "adds-curl".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![drv("curl", Some("8.0"))],
            replaces: vec![],
            description: None,
        };
        let b = Overlay {
            name: "patches-hello".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![],
            replaces: vec![Replacement {
                upstream_name: "hello".into(),
                with: drv("hello-patched", Some("2.12.1-p1")),
            }],
            description: None,
        };
        let composed = compose(&[a.clone(), b.clone()]).unwrap();

        // Applying the composition = applying the chain.
        let via_compose = apply(&composed, &base());
        let via_chain = apply_chain(&[a, b], &base());
        assert_eq!(via_compose, via_chain);
    }

    #[test]
    fn compose_later_wins_on_add_conflict() {
        let a = Overlay {
            name: "a".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![drv("curl", Some("8.0"))],
            replaces: vec![],
            description: None,
        };
        let b = Overlay {
            name: "b".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![drv("curl", Some("8.5"))],
            replaces: vec![],
            description: None,
        };
        let composed = compose(&[a, b]).unwrap();
        let curl = composed.adds.iter().find(|d| d.name == "curl").unwrap();
        assert_eq!(curl.version.as_deref(), Some("8.5"));
    }

    #[test]
    fn compose_later_wins_on_replace_conflict() {
        let a = Overlay {
            name: "a".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![],
            replaces: vec![Replacement {
                upstream_name: "hello".into(),
                with: drv("hello-v1", Some("1.0")),
            }],
            description: None,
        };
        let b = Overlay {
            name: "b".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![],
            replaces: vec![Replacement {
                upstream_name: "hello".into(),
                with: drv("hello-v2", Some("2.0")),
            }],
            description: None,
        };
        let composed = compose(&[a, b]).unwrap();
        let r = composed
            .replaces
            .iter()
            .find(|r| r.upstream_name == "hello")
            .unwrap();
        assert_eq!(r.with.name, "hello-v2");
    }

    #[test]
    fn compose_rejects_heterogeneous_targets() {
        let a = Overlay {
            name: "pkg".into(),
            target: OverlayTarget::PackageSet,
            adds: vec![],
            replaces: vec![],
            description: None,
        };
        let b = Overlay {
            name: "mod".into(),
            target: OverlayTarget::Module,
            adds: vec![],
            replaces: vec![],
            description: None,
        };
        let err = compose(&[a, b]).unwrap_err();
        assert!(matches!(err, ComposeError::TargetMismatch { .. }));
    }
}
