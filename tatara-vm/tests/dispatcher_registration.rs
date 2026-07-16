//! Verify tatara-vm registers Hypervisor into the gen-platform
//! fleet catalog. tatara is the TENTH consumer class adopting
//! the typed-dispatcher catamorphism — joins gen / caixa /
//! wasm-platform / cofre / shigoto / engenho / magma / kura /
//! pangea.
//!
//! Hypervisor enumerates the available hypervisor backends for
//! tatara-vm guests:
//!   - Vfkit       (Apple Virtualization.framework, Linux guests)
//!   - VfkitDarwin (Apple Virtualization.framework, Darwin guests)
//!   - Qemu        (KVM/HVF portable fallback)
//!   - Kasou       (pleme-io Rust-native VZ wrapper, in-process)
//!   - Libkrun     (pleme-io Rust-native libkrun wrapper, in-process — the
//!                  default máquina engine, see theory/MAQUINA.md)

use gen_platform::{catalog, TypedDispatcherTrait};
use tatara_vm::Hypervisor;

#[test]
fn hypervisor_registers_into_fleet_catalog() {
    let entry = catalog::by_label("tatara.hypervisor")
        .expect("tatara-vm must register Hypervisor into the fleet catalog");
    assert_eq!(entry.label, "tatara.hypervisor");
    assert_eq!((entry.variant_count)(), 5);
}

#[test]
fn hypervisor_variant_kinds_kebab() {
    assert_eq!(
        Hypervisor::variant_kinds(),
        vec!["vfkit", "vfkit-darwin", "qemu", "kasou", "libkrun"]
    );
}

#[test]
fn hypervisor_round_trip() {
    use std::str::FromStr;
    for variant in [
        Hypervisor::Vfkit,
        Hypervisor::VfkitDarwin,
        Hypervisor::Qemu,
        Hypervisor::Kasou,
        Hypervisor::Libkrun,
    ] {
        let k = variant.discriminant();
        let back = Hypervisor::from_str(k)
            .unwrap_or_else(|_| panic!("FromStr must accept own discriminant: {k}"));
        assert_eq!(back.discriminant(), variant.discriminant());
    }
}

#[test]
fn hypervisor_display_delegates_to_discriminant() {
    assert_eq!(Hypervisor::Vfkit.to_string(), "vfkit");
    assert_eq!(Hypervisor::VfkitDarwin.to_string(), "vfkit-darwin");
    assert_eq!(Hypervisor::Kasou.to_string(), "kasou");
    assert_eq!(Hypervisor::Libkrun.to_string(), "libkrun");
}

#[test]
fn hypervisor_predicates() {
    let qemu = Hypervisor::Qemu;
    assert!(qemu.is_qemu());
    assert!(!qemu.is_vfkit());
    assert!(!qemu.is_vfkit_darwin());
    assert!(!qemu.is_kasou());
    assert!(!qemu.is_libkrun());

    let libkrun = Hypervisor::Libkrun;
    assert!(libkrun.is_libkrun());
    assert!(!libkrun.is_kasou());
}

#[test]
fn hypervisor_const_fn_in_const_context() {
    const IS_VFKIT: bool = Hypervisor::Vfkit.is_vfkit();
    const KIND: &str = Hypervisor::Vfkit.discriminant();
    assert!(IS_VFKIT);
    assert_eq!(KIND, "vfkit");
}
