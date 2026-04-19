//! Smoke test: `LayeredTransport::fetch` on a local store path that
//! already exists returns it via the `LocalTransport` branch. Proves
//! the composition works end-to-end without network.

use tatara_build_remote::{BuildRef, BuildTransport, BuildTransportChain};

#[test]
fn local_fallback_resolves_existing_store_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("existing");
    std::fs::write(&path, b"x").unwrap();

    let chain = BuildTransportChain::local_only();
    let layered = chain.to_layered();
    let r = BuildRef::StorePath(path.display().to_string());
    let out = layered.fetch(&r).expect("layered fetch");
    assert_eq!(out.0, path.display().to_string());
}

#[test]
fn full_chain_short_circuits_on_existing_path() {
    // Attic transport will report "only satisfies StorePath" or "not on
    // PATH" — both are errors that advance the layered chain. Local
    // will hit the fast path (file exists) and return.
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("existing-2");
    std::fs::write(&path, b"x").unwrap();

    let chain = BuildTransportChain::quero_lol();
    let layered = chain.to_layered();
    assert_eq!(layered.transports.len(), 3);

    let r = BuildRef::StorePath(path.display().to_string());
    let out = layered.fetch(&r).expect("chain walks to local");
    assert_eq!(out.0, path.display().to_string());
}
