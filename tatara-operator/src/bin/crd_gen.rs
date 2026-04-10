use kube::CustomResourceExt;

fn main() {
    print!(
        "{}",
        serde_yaml::to_string(&tatara_operator::crds::nix_build::NixBuild::crd()).unwrap()
    );
    println!("---");
    print!(
        "{}",
        serde_yaml::to_string(&tatara_operator::crds::nix_build_pool::NixBuildPool::crd()).unwrap()
    );
    println!("---");
    print!(
        "{}",
        serde_yaml::to_string(&tatara_operator::crds::flake_source::FlakeSource::crd()).unwrap()
    );
    println!("---");
    print!(
        "{}",
        serde_yaml::to_string(&tatara_operator::crds::flake_org::FlakeOrg::crd()).unwrap()
    );
}
