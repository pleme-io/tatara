//! Prebuilt WASI preview 2 components for embedders' tests.
//!
//! Built from `fixtures/probe` and `fixtures/greeter` with a toolchain that
//! carries the `wasm32-wasip2` target, e.g.
//! `cargo build --release --target wasm32-wasip2 --manifest-path fixtures/probe/Cargo.toml`,
//! and copied next to their sources. Each source's header states what the
//! component does.

/// A `wasi:cli/command` driven by its first argument: `serve <port>`,
/// `count <ms>`, `exit <code>`, `spin`, `grow <mib>`, `connect <host:port>`, `bind <port>`,
/// `read <path>`, `write <path>`.
pub const PROBE: &[u8] = include_bytes!("../fixtures/probe.wasm");

/// A `wasi:http/proxy` answering `hello from <path>`, and `GET /fetch/<authority>`
/// with an outgoing request to that authority.
pub const GREETER: &[u8] = include_bytes!("../fixtures/greeter.wasm");
