//! End-to-end: compose a VmSpec + LinuxRootfs, realize the rootfs through
//! `/nix/store`. Live-Nix tests are `#[ignore]`-gated (run with `--ignored`).

use tatara_lisp::{domain::TataraDomain, read};
use tatara_nix::{NixStoreRealizer, Realizer};
use tatara_vm::{LinuxRootfs, VmSpec};

fn sample_init_config() -> &'static str {
    r#"(definit
         :name "plex-boot"
         :services ((:name "demo"
                     :exec "/bin/busybox sh -c 'while true; do echo tatara; sleep 5; done'")))"#
}

#[test]
fn rootfs_derivation_embeds_init_config() {
    // `hello` stands in for tatara-init in this hermetic test — we only
    // assert the emitted expression shape, not that it builds.
    let r = LinuxRootfs::new(
        "${pkgs.hello}/bin/hello",
        sample_init_config(),
    );
    let d = r.derivation();
    let expr = d.nix_expr.unwrap();
    assert!(expr.contains(":name \"plex-boot\""));
    assert!(expr.contains("(:name \"demo\""));
    assert!(expr.contains("root/sbin/init"));
    assert!(expr.contains("initrd.cpio.gz"));
}

#[test]
fn vm_spec_parses_with_rootfs_reference() {
    let forms = read(
        r#"(defvm
             :name       "plex-guest"
             :cpus       2
             :memory-mib 1024
             :hypervisor (:kind "Vfkit")
             :kernel     (:kind "Bridge" :attr_path "linuxPackages.kernel")
             :rootfs     (:kind "Bridge" :attr_path "placeholder-initrd")
             :cmdline    ("console=hvc0" "init=/bin/tatara-init"))"#,
    )
    .unwrap();
    let v = VmSpec::compile_from_sexp(&forms[0]).unwrap();
    assert_eq!(v.name, "plex-guest");
}

#[test]
#[ignore]
fn rootfs_realizes_through_live_nix_store() {
    // Builds a real initrd in /nix/store using `hello` as a stand-in for
    // tatara-init. Proves the runCommand expression is valid Nix + emits
    // an initrd.cpio.gz + rootfs/ tree. Skips busybox so this runs on a
    // Darwin host (busybox is Linux-only; real guest builds target
    // aarch64-linux through linux-builder or on a Linux host).
    let r = LinuxRootfs::new(
        "${pkgs.hello}/bin/hello",
        sample_init_config(),
    )
    .without_busybox()
    .with_name("tatara-rootfs-test");
    let d = r.derivation();

    let realizer = NixStoreRealizer::new();
    let art = realizer.realize(&d).expect("nix build of initrd should succeed");
    assert!(art.path.starts_with("/nix/store"));
    assert!(
        art.path.join("initrd.cpio.gz").exists(),
        "expected initrd.cpio.gz at {}",
        art.path.display()
    );
    assert!(art.path.join("rootfs").exists());
    assert!(art.path.join("rootfs/bin/tatara-init").exists());
    assert!(art.path.join("rootfs/sbin/init").is_symlink());
    let init_lisp = std::fs::read_to_string(art.path.join("rootfs/etc/tatara/init.lisp")).unwrap();
    assert!(init_lisp.contains(":name \"plex-boot\""));
}
