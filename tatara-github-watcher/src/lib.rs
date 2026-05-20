//! tatara-github-watcher — GitHub webhook receiver.
//!
//! Watches an entire GitHub organization (or several) by receiving
//! org-level webhooks and translating PR + push + branch events into
//! `EphemeralAllocation` CRs that the pool reconciler routes to a
//! matching `EphemeralPool`.
//!
//! Modules:
//! - `verify` — HMAC-SHA256 signature verification (GitHub's
//!   `X-Hub-Signature-256` header). Constant-time comparison.
//! - `event` — typed GitHub event shapes (just the fields we read).
//! - `allocation_factory` — typed translator: GitHub PR event →
//!   `EphemeralAllocation` spec. Pure function, fully unit-tested.
//! - `handler` — axum HTTP handler that verifies signature, dispatches
//!   on event type, applies via kube-rs.
//! - `config` — typed config struct loaded from env / CLI flags.

#![warn(rust_2018_idioms)]

pub mod allocation_factory;
pub mod config;
pub mod event;
pub mod handler;
pub mod verify;

pub use allocation_factory::{allocation_name, build_allocation, FactoryError};
pub use config::WatcherConfig;
pub use event::{EventKind, PrAction, PullRequestEvent, PushEvent};
pub use verify::{verify_signature, VerifyError};
