//! Test infrastructure for tatara.
//!
//! Provides an in-memory mock of the tatara API that doesn't require
//! Raft consensus, gossip membership, or any networking. This enables:
//!
//! - **API integration tests**: Full HTTP round-trip tests against a virtualized tatara server
//! - **Forge workflow tests**: End-to-end forge lifecycle testing (submit -> schedule -> release)
//! - **Scheduler tests**: Property-based testing of scheduling invariants
//!
//! # Usage
//!
//! ```rust,no_run
//! use tatara_testing::TestServer;
//!
//! #[tokio::test]
//! async fn test_submit_job() {
//!     let server = TestServer::new();
//!     let response = server.post("/api/v1/jobs", &job_spec).await;
//!     assert_eq!(response.status(), 200);
//! }
//! ```

pub mod fixtures;
pub mod server;
pub mod store;

pub use fixtures::*;
pub use server::TestServer;
pub use store::InMemoryStore;
