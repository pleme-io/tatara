//! tatara-kube: Nix-native Kubernetes reconciler.
//!
//! Replaces FluxCD with direct Server-Side Apply from Nix flake evaluation output.
//! Evaluates `kubeResources.<system>.clusters.<name>` flake outputs, diffs against
//! live cluster state, and applies via Kubernetes Server-Side Apply.

pub mod apply;
pub mod cluster;
pub mod config;
pub mod error;
pub mod health;
pub mod helm;
pub mod metrics;
pub mod nix_eval;
pub mod ordering;
pub mod prune;
pub mod reconciler;
pub mod resource;

pub use config::KubeConfig;
pub use error::KubeError;
pub use reconciler::KubeReconciler;
pub use resource::{DesiredState, ManagedResource, ResourceIdentity};
