//! Service catalog registry — consul-like service discovery for tatara.
//!
//! Manages service registrations backed by in-memory state.
//! The registry is the local view; Raft replication and gossip propagation
//! happen at the cluster layer.

pub mod registry;
