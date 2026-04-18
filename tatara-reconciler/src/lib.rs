//! tatara-reconciler — FluxCD-adjacent Kubernetes controller.
//!
//! **Not a replacement for FluxCD.** The reconciler *emits* FluxCD
//! Kustomizations and HelmReleases; source-controller + kustomize-controller +
//! helm-controller do the actual apply. We layer:
//!
//! 1. Unix process lifecycle over every reconciled object (`tatara-process`)
//! 2. Three-pillar BLAKE3 attestation over each convergence cycle (`tatara-core`)
//! 3. Lattice-ordered compliance/classification gating (`tatara-lattice`)
//! 4. Multi-surface intent rendering: Nix, Lisp, Flux-passthrough, Container
//!
//! The top-level reconcile function is a phase dispatcher; one handler per
//! `ProcessPhase`, all side-effect-free w.r.t. Flux until the RENDER step.

pub mod boundary;
pub mod context;
pub mod controller;
pub mod patch;
pub mod phase_machine;
pub mod pid;
pub mod render;
pub mod signals;
pub mod ssapply;
pub mod table_controller;

pub use context::Context;
pub use controller::{error_policy, reconcile};
