//! `OverlayPackageSet` — one set composed over another. `top` wins where it
//! defines a package; `base` fills in the rest. Names enumerate the union.

use crate::set::{PackageLookup, PackageSet, PackageSetError};

pub struct OverlayPackageSet {
    pub base: Box<dyn PackageSet>,
    pub top: Box<dyn PackageSet>,
    pub label: String,
}

impl OverlayPackageSet {
    pub fn new(base: Box<dyn PackageSet>, top: Box<dyn PackageSet>) -> Self {
        let lbl = format!("{} <- {}", base.label(), top.label());
        Self {
            base,
            top,
            label: lbl,
        }
    }
}

impl PackageSet for OverlayPackageSet {
    fn get(&self, name: &str) -> Result<PackageLookup, PackageSetError> {
        match self.top.get(name)? {
            Some(d) => Ok(Some(d)),
            None => self.base.get(name),
        }
    }

    fn names(&self) -> Vec<String> {
        let mut all = self.base.names();
        for n in self.top.names() {
            if !all.contains(&n) {
                all.push(n);
            }
        }
        all.sort();
        all
    }

    fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NixpkgsBridge;

    #[test]
    fn top_wins_on_overlap_base_fills_rest() {
        let base = NixpkgsBridge::new()
            .with_names(vec!["hello".into(), "bash".into()])
            .with_label("base");
        let top = NixpkgsBridge::new()
            .with_pkg_set("import ./my-nixpkgs.nix {}")
            .with_names(vec!["bash".into(), "zsh".into()])
            .with_label("top");
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));

        let bash = overlay.get("bash").unwrap().unwrap();
        // bash comes from top, so its pkg_set is the overlay expr
        assert_eq!(
            bash.bridge.unwrap().pkg_set.as_deref(),
            Some("import ./my-nixpkgs.nix {}")
        );

        let hello = overlay.get("hello").unwrap().unwrap();
        // hello comes from base (default pkg_set → None → "import <nixpkgs> {}")
        assert!(hello.bridge.unwrap().pkg_set.is_none());

        let mut ns = overlay.names();
        ns.sort();
        assert_eq!(ns, vec!["bash", "hello", "zsh"]);
    }
}
