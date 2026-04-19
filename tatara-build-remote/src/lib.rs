//! `tatara-build-remote` — layered Nix build transport for tatara guests.
//!
//! Takes a `BuildRef` (flake + attr, raw Nix expression, store path, or
//! OCI image) and resolves it to a concrete `StorePath` using a
//! priority-ordered chain of transports. **First match wins.** Default
//! chain:
//!
//! 1. **Attic cache** — pulls from a shared Attic instance (e.g.
//!    `quero.lol`). Fastest path when the artifact is already cached.
//! 2. **ssh-ng remote builder** — submits to a remote Nix builder over
//!    `ssh-ng://`. Used when Attic misses and the local machine can't
//!    or shouldn't build (cross-arch, resource constrained, etc.).
//! 3. **Local** — `nix build` on the host. Last resort.
//!
//! Any transport declared absent in the spec is skipped. If all declared
//! transports fail, `BuildError::AllTransportsFailed` bubbles up and
//! hospedeiro refuses to boot the guest — we fail closed.
//!
//! # Status
//!
//! **Phase H.5 landed.** `AtticTransport`, `SshRemoteTransport`, and
//! `LocalTransport` all ship in `transports.rs`.
//! `BuildTransportChain::to_layered()` composes them into a priority-
//! ordered `LayeredTransport` driven by `(defguest …)`'s `:build-on`
//! keyword.
//!
//! # Why layered, not single-target
//!
//! The fleet at `quero.lol` has a shared Attic cache *and* an ssh-ng
//! builder pool. Cache hits are free; builds are expensive. Layering
//! lets the common case (pleme-io team members pulling pre-built
//! artifacts) skip the slow path entirely. Keys + SSH config come from
//! the cid node's `pangea-builder.nix` — no new auth plumbing.

#![forbid(unsafe_code)]

pub mod transports;

pub use transports::{AtticTransport, LocalTransport, SshRemoteTransport};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A reference to something that becomes a Nix store path.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum BuildRef {
    /// `nix build github:pleme-io/tatara-os#kernel`
    Flake { url: String, attr: String },
    /// `nix build --expr '(import ./default.nix).thing'`
    Nix { expr: String },
    /// Already in the store — skip build entirely.
    StorePath(String),
    /// An OCI image to import via `skopeo`/`docker load`/`nix2container`.
    Oci { image: String, tag: String },
}

/// Declarative transport chain. A `None` field means "don't try this transport".
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BuildTransportChain {
    /// Attic cache name. E.g. `"quero.lol"`.
    pub attic: Option<String>,
    /// ssh-ng builder URI. E.g. `"ssh://builder.quero.lol"`.
    pub remote: Option<String>,
    /// Fall back to local `nix build`. Default `true`.
    #[serde(default = "yes")]
    pub local: bool,
}

fn yes() -> bool {
    true
}

impl BuildTransportChain {
    /// The pleme-io default — Attic, ssh-ng, local, all three against quero.lol.
    #[must_use]
    pub fn quero_lol() -> Self {
        Self {
            attic: Some("quero.lol".into()),
            remote: Some("ssh://builder.quero.lol".into()),
            local: true,
        }
    }

    /// Resolve this declarative chain into a concrete `LayeredTransport`
    /// that can actually `fetch()` a `BuildRef`. The order is always
    /// Attic → ssh-ng → local; missing transports are skipped.
    #[must_use]
    pub fn to_layered(&self) -> LayeredTransport {
        let mut transports: Vec<Box<dyn BuildTransport + Send + Sync>> = Vec::new();
        if let Some(cache) = &self.attic {
            transports.push(Box::new(AtticTransport::new(cache.clone())));
        }
        if let Some(ssh) = &self.remote {
            transports.push(Box::new(SshRemoteTransport::new(ssh.clone())));
        }
        if self.local {
            transports.push(Box::new(LocalTransport::default()));
        }
        LayeredTransport { transports }
    }

    /// Local only — no remote anything.
    #[must_use]
    pub fn local_only() -> Self {
        Self {
            attic: None,
            remote: None,
            local: true,
        }
    }

    /// Remote only — refuse local fallback.
    #[must_use]
    pub fn remote_only(ssh: impl Into<String>) -> Self {
        Self {
            attic: None,
            remote: Some(ssh.into()),
            local: false,
        }
    }
}

/// The operation-level transport trait. Phase H.5 implements
/// `AtticTransport`, `SshRemoteTransport`, and `LocalTransport`.
pub trait BuildTransport {
    /// Fetch / build the artifact, returning a store path.
    ///
    /// # Errors
    /// Returns `BuildError` on any failure. The `LayeredTransport`
    /// swallows individual errors and advances to the next transport.
    fn fetch(&self, reference: &BuildRef) -> Result<StorePath, BuildError>;
}

/// A Nix store path — content-addressed handle.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorePath(pub String);

/// Layered transport — tries each child in order, returns the first success.
pub struct LayeredTransport {
    pub transports: Vec<Box<dyn BuildTransport + Send + Sync>>,
}

impl BuildTransport for LayeredTransport {
    fn fetch(&self, r: &BuildRef) -> Result<StorePath, BuildError> {
        let mut last_err = None;
        for t in &self.transports {
            match t.fetch(r) {
                Ok(p) => return Ok(p),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or(BuildError::AllTransportsFailed))
    }
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("attic: {0}")]
    Attic(String),
    #[error("remote ssh-ng: {0}")]
    Remote(String),
    #[error("local nix build: {0}")]
    Local(String),
    #[error("all transports failed")]
    AllTransportsFailed,
    #[error("transport not configured: {0}")]
    NotConfigured(String),
}

/// Phase marker. Bumped by each phase that lands a change.
pub const CRATE_STATUS: &str = "phase-h5";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_ref_json_round_trip() {
        let r = BuildRef::Flake {
            url: "github:pleme-io/tatara-os".into(),
            attr: "kernel".into(),
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: BuildRef = serde_json::from_str(&j).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn quero_lol_preset_is_full_chain() {
        let c = BuildTransportChain::quero_lol();
        assert_eq!(c.attic.as_deref(), Some("quero.lol"));
        assert_eq!(c.remote.as_deref(), Some("ssh://builder.quero.lol"));
        assert!(c.local);
    }

    #[test]
    fn remote_only_refuses_local() {
        let c = BuildTransportChain::remote_only("ssh://foo.example");
        assert!(!c.local);
        assert_eq!(c.remote.as_deref(), Some("ssh://foo.example"));
    }

    #[test]
    fn local_only_has_no_remote() {
        let c = BuildTransportChain::local_only();
        assert!(c.attic.is_none());
        assert!(c.remote.is_none());
        assert!(c.local);
    }

    #[test]
    fn quero_lol_to_layered_builds_three_transports() {
        let chain = BuildTransportChain::quero_lol();
        let layered = chain.to_layered();
        assert_eq!(layered.transports.len(), 3);
    }

    #[test]
    fn remote_only_to_layered_has_one_transport() {
        let chain = BuildTransportChain::remote_only("ssh://foo.example");
        let layered = chain.to_layered();
        assert_eq!(layered.transports.len(), 1);
    }

    #[test]
    fn local_only_to_layered_has_one_transport() {
        let chain = BuildTransportChain::local_only();
        let layered = chain.to_layered();
        assert_eq!(layered.transports.len(), 1);
    }

    #[test]
    fn empty_chain_to_layered_has_no_transports() {
        let chain = BuildTransportChain {
            attic: None,
            remote: None,
            local: false,
        };
        let layered = chain.to_layered();
        assert!(layered.transports.is_empty());
    }
}
