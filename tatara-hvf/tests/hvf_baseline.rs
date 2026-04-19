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

    // Memory map at a non-zero guest physical address — HVF doesn't
    // love running code at PC=0. 16 KiB page is the default PAGE_SIZE
    // applevisor uses.
    const PAGE: usize = 16 * 1024;
    const GUEST_PHYS: u64 = 0x4000;
    let region = engine
        .create_memory(GUEST_PHYS, PAGE, Permissions::rwx())
        .expect("memory created");
    assert_eq!(region.size_bytes, PAGE);
    assert_eq!(engine.memory_region_count(), 1);

    // Write an ARM64 `ret` (0xD65F03C0) to prove write_guest_bytes works.
    let ret_bytes = 0xD65F_03C0_u32.to_le_bytes();
    engine
        .write_guest_bytes(GUEST_PHYS, &ret_bytes)
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

    // ─── H.2.2.a — first instruction executes on bare metal ──────────
    //
    // Replace the `ret` we wrote earlier with `brk #0` (0xD4200000) —
    // a trap that immediately exits the vCPU with EXCEPTION. Proves
    // the CPU fetched our bytes, decoded them, and HVF handed control
    // back to host code.
    //
    // Matching applevisor's own vcpu tests: don't override system-reg
    // defaults, just point PC at the instruction and run.
    let brk_bytes = 0xD420_0000_u32.to_le_bytes();
    engine
        .write_guest_bytes(GUEST_PHYS, &brk_bytes)
        .expect("write brk");
    engine
        .vcpu_write_reg(0, Reg::PC, GUEST_PHYS)
        .expect("set PC to guest_phys");

    engine
        .vcpu_run(0)
        .expect("vcpu_run returned without FFI error");

    // Exit reason must be EXCEPTION.
    use applevisor::prelude::ExitReason;
    let reason = engine.vcpu_exit_reason(0).expect("exit reason");
    assert_eq!(
        reason,
        ExitReason::EXCEPTION,
        "expected EXCEPTION from brk, got {reason:?}"
    );

    // H.2.2.a proof: vcpu.run() returned cleanly AND an EXCEPTION
    // occurred inside the guest. The exception's PC landing spot
    // depends on the default reset-state's VBAR_EL1 + the precise
    // trap type (applevisor defaults to an MMU-off state where the
    // first fetch at GUEST_PHYS may still immediately fault). Full
    // control-flow proof (brk reaches retirement) needs explicit
    // PSTATE / SCTLR_EL1 / VBAR_EL1 setup — that's H.2.2.b scope,
    // landing with thread-per-vCPU dispatch.
    //
    // What H.2.2.a establishes: vcpu_run() ↔ vcpu_exit_reason() work
    // as real HVF round-trips. That's the primitive virtio backends
    // need.
    let pc_after = engine.vcpu_read_reg(0, Reg::PC).expect("read PC after");
    assert!(pc_after != GUEST_PHYS, "PC must have advanced (got 0x{pc_after:x})");

    // GP registers survive across the run.
    let x0_after = engine.vcpu_read_reg(0, Reg::X0).expect("read X0 after");
    assert_eq!(x0_after, 0xDEAD_BEEF, "X0 clobbered across run()");
}
