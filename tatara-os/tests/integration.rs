//! End-to-end integration — parse a `(defsystem …)` Lisp form, build the
//! closure, realize selected derivations through both backends.
//!
//! The live-Nix tests are `#[ignore]` so CI stays hermetic. Run explicitly:
//!   cargo test -p tatara-os -- --ignored

use tatara_lisp::{domain::TataraDomain, read};
use tatara_nix::{InProcessRealizer, MultiSynthesizer, NixStoreRealizer, Realizer};
use tatara_os::{SystemClosure, SystemConfig, SystemSynthesizer};
use tatara_pkgs::NixpkgsBridge;

fn sample_source() -> &'static str {
    r#"(defsystem
         :hostname   "plex-test"
         :system     "x86_64-linux"
         :kernel     (:kind "Bridge" :attr_path "hello")
         :services   ((:name "demo" :exec "/bin/demo")))"#
}

#[test]
fn lisp_roundtrip_to_realized_etc_in_process() {
    // Parse defsystem form → typed SystemConfig.
    let forms = read(sample_source()).unwrap();
    let cfg = SystemConfig::compile_from_sexp(&forms[0]).unwrap();
    assert_eq!(cfg.hostname, "plex-test");
    assert_eq!(cfg.services.len(), 1);

    // Build the closure.
    let pkgs = NixpkgsBridge::new();
    let closure = SystemClosure::from_config(&cfg, &pkgs).unwrap();

    // Realize the /etc derivation hermetically (no Nix needed).
    let store = tempfile::tempdir().unwrap();
    let realizer = InProcessRealizer::new(store.path());
    let art = realizer.realize(&closure.etc).unwrap();

    // The derivation writes hostname into $out/hostname.
    let hostname_file = art.path.join("hostname");
    assert!(
        hostname_file.exists(),
        "expected {}",
        hostname_file.display()
    );
    let content = std::fs::read_to_string(&hostname_file).unwrap();
    assert_eq!(content.trim(), "plex-test");
}

#[test]
fn lisp_roundtrip_to_realized_activation_script() {
    let forms = read(sample_source()).unwrap();
    let cfg = SystemConfig::compile_from_sexp(&forms[0]).unwrap();
    let pkgs = NixpkgsBridge::new();
    let closure = SystemClosure::from_config(&cfg, &pkgs).unwrap();

    let store = tempfile::tempdir().unwrap();
    let realizer = InProcessRealizer::new(store.path());
    let art = realizer.realize(&closure.activation).unwrap();
    let script = std::fs::read_to_string(art.path.join("activate")).unwrap();
    assert!(script.starts_with("#!/bin/sh"));
    assert!(script.contains("echo 'plex-test' > /etc/hostname"));
    // Default init is tatara-init — activation signals the running
    // supervisor with SIGHUP, no systemctl.
    assert!(script.contains("tatara-init — PID 1"));
    assert!(script.contains("kill -HUP"));
}

#[test]
fn synthesizer_emits_full_artifact_set() {
    let forms = read(sample_source()).unwrap();
    let cfg = SystemConfig::compile_from_sexp(&forms[0]).unwrap();
    let pkgs = NixpkgsBridge::new();
    let synth = SystemSynthesizer::new(&pkgs).with_prefix("out");
    let arts = synth.generate_all(&cfg);

    let paths: Vec<&str> = arts.iter().map(|a| a.path.as_str()).collect();
    assert!(paths.contains(&"out/system.json"));
    assert!(paths.contains(&"out/closure.json"));
    assert!(paths.contains(&"out/activate.sh"));
    assert!(paths.contains(&"out/manifest.txt"));

    // The manifest embeds a row for each closure derivation.
    let manifest = arts
        .iter()
        .find(|a| a.path.ends_with("/manifest.txt"))
        .unwrap();
    // kernel + etc + activation + 1 service + profile = 5 rows (+ header)
    let rows = manifest
        .content
        .lines()
        .filter(|l| !l.starts_with('#'))
        .count();
    assert_eq!(rows, 5, "manifest rows: {}", manifest.content);
}

#[test]
#[ignore]
fn bridged_kernel_realizes_via_live_nix() {
    // Short-circuit: use `hello` as our "kernel" so the live build stays fast
    // (no actual kernel compile). Proves SystemClosure's bridged derivation
    // realizes through `/nix/store` end-to-end.
    let forms = read(sample_source()).unwrap();
    let cfg = SystemConfig::compile_from_sexp(&forms[0]).unwrap();
    let pkgs = NixpkgsBridge::new();
    let closure = SystemClosure::from_config(&cfg, &pkgs).unwrap();

    let r = NixStoreRealizer::new();
    let art = r.realize(&closure.kernel).unwrap();
    assert!(
        art.path.to_string_lossy().starts_with("/nix/store"),
        "expected /nix/store path, got {}",
        art.path.display()
    );
    // hello is packaged as a Nix output containing bin/hello.
    assert!(art.path.join("bin/hello").exists());
}

#[test]
#[ignore]
fn hermetic_etc_realizes_via_live_nix() {
    // /etc is Inline-sourced + hermetic builder; NixStoreRealizer with stdenv
    // PATH injection should produce it inside /nix/store.
    let forms = read(sample_source()).unwrap();
    let cfg = SystemConfig::compile_from_sexp(&forms[0]).unwrap();
    let pkgs = NixpkgsBridge::new();
    let closure = SystemClosure::from_config(&cfg, &pkgs).unwrap();

    let r = NixStoreRealizer::new();
    let art = r.realize(&closure.etc).unwrap();
    assert!(art.path.to_string_lossy().starts_with("/nix/store"));
    assert!(art.path.join("hostname").exists());
    let content = std::fs::read_to_string(art.path.join("hostname")).unwrap();
    assert_eq!(content.trim(), "plex-test");
}
