//! HVF baseline smoke test. Gated `#[ignore]` because running it
//! requires the `com.apple.security.hypervisor` entitlement on the
//! test binary. Unignore + codesign locally to exercise:
//!
//! ```bash
//! cat > /tmp/hv.plist <<'EOF'
//! <?xml version="1.0" encoding="UTF-8"?>
//! <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
//!   "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
//! <plist version="1.0">
//! <dict>
//!   <key>com.apple.security.hypervisor</key><true/>
//! </dict>
//! </plist>
//! EOF
//! cargo test -p tatara-hvf --test hvf_baseline --no-run
//! codesign --entitlements /tmp/hv.plist --force --sign - \
//!   target/debug/deps/hvf_baseline-*
//! cargo test -p tatara-hvf --test hvf_baseline -- --ignored
//! ```
//!
//! One test, not two — Hypervisor.framework refuses to honor multiple
//! `hv_vm_create` calls from the same process (returns "owning resource
//! is busy"). So all lifecycle checks happen inside a single
//! `HvfEngine::new()` call.

#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

use tatara_hvf::{HvfEngine, HvfError, Permissions};

#[test]
#[ignore = "requires com.apple.security.hypervisor entitlement"]
fn hvf_lifecycle_baseline() {
    // VM create
    let mut engine = match HvfEngine::new() {
        Ok(e) => e,
        Err(HvfError::MissingEntitlement) => {
            panic!("test binary not entitled for hv — codesign with entitlements plist first");
        }
        Err(e) => panic!("VM create failed: {e:?}"),
    };

    // Memory map
    const PAGE: usize = 16 * 1024;
    let region = engine
        .create_memory(0, PAGE, Permissions::rwx())
        .expect("memory created");
    assert_eq!(region.size_bytes, PAGE);
    assert_eq!(engine.memory_region_count(), 1);

    // Write an ARM64 `ret` (0xD65F03C0) to prove write_guest_bytes works.
    let ret_bytes = 0xD65F_03C0_u32.to_le_bytes();
    engine
        .write_guest_bytes(0, &ret_bytes)
        .expect("guest write succeeded");

    // vCPU create
    let vcpu_idx = engine.create_vcpu().expect("vcpu created");
    assert_eq!(vcpu_idx, 0);
    assert_eq!(engine.vcpu_count(), 1);

    // Register read/write
    use applevisor::prelude::Reg;
    engine
        .vcpu_write_reg(0, Reg::X0, 0xDEAD_BEEF)
        .expect("register write");
    let x0 = engine.vcpu_read_reg(0, Reg::X0).expect("register read");
    assert_eq!(x0, 0xDEAD_BEEF);

    // NOTE: HVF binds each vCPU to the thread that created it. Multiple
    // vCPUs on one thread return "owning resource is busy". Thread-per-
    // vCPU dispatch is H.2.2 scope — once hospedeiro spawns worker
    // threads, we revisit this assertion.

    // Out-of-range vCPU index is a typed error, not a panic.
    let err = engine.vcpu_read_reg(99, Reg::X0).unwrap_err();
    assert!(matches!(err, HvfError::Register(_)));
}
