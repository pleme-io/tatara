//! `tatara-crd-gen` — emit `CustomResourceDefinition` YAML for the tatara Process CRDs.
//!
//! ```sh
//! tatara-crd-gen > chart/tatara/templates/crds/all.yaml
//! ```

use kube::CustomResourceExt;

use tatara_process::prelude::{Process, ProcessTable};

fn main() {
    let crds = [Process::crd(), ProcessTable::crd()];
    for crd in crds {
        let yaml = serde_yaml::to_string(&crd).expect("CRD is serializable");
        println!("---\n{yaml}");
    }
}
