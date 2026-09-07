//! `tatara-crd-gen` — emit `CustomResourceDefinition` YAML for the tatara Process CRDs.
//!
//! ```sh
//! tatara-crd-gen > chart/tatara/templates/crds/all.yaml
//! ```
//!
//! The four-CRD closed set rides through the ONE substrate primitive
//! [`tatara_reconciler::known_crd::KnownCrd`] — pre-lift the emit
//! surface hand-authored the 4-entry inline array `[Process::crd(),
//! ProcessTable::crd(), EphemeralPool::crd(), EphemeralAllocation::crd()]`,
//! sibling to the SAME 4-arm dispatcher in `tatara-check`'s
//! `check_crd_in_sync` past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
//! trigger. Post-lift both bins iterate `KnownCrd::ALL` and dispatch
//! through `emit_crd_yaml`; a fifth CRD lands at ONE variant on the
//! closed set and both emit paths inherit it mechanically.

use tatara_reconciler::known_crd::KnownCrd;

fn main() {
    for k in KnownCrd::ALL {
        let yaml = k.emit_crd_yaml().expect("CRD is serializable");
        println!("---\n{yaml}");
    }
}
