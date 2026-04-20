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

    /// A stub PackageSet that errors on every lookup — used to test
    /// that errors from either side (top or base) propagate.
    struct Erroring {
        reason: String,
    }
    impl PackageSet for Erroring {
        fn get(&self, _name: &str) -> Result<PackageLookup, PackageSetError> {
            Err(PackageSetError::Backend(self.reason.clone()))
        }
        fn names(&self) -> Vec<String> {
            vec![]
        }
        fn label(&self) -> &str {
            &self.reason
        }
    }

    /// A stub PackageSet that returns None for every lookup — lets us
    /// build a genuinely empty overlay (NixpkgsBridge treats an empty
    /// `with_names` vector as an OPEN universe and still resolves
    /// anything, so it can't stand in for an empty closed set).
    struct Empty {
        label: String,
    }
    impl PackageSet for Empty {
        fn get(&self, _name: &str) -> Result<PackageLookup, PackageSetError> {
            Ok(None)
        }
        fn names(&self) -> Vec<String> {
            vec![]
        }
        fn label(&self) -> &str {
            &self.label
        }
    }

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

    #[test]
    fn label_composition_is_base_arrow_top() {
        // Pinned: the composed label is "{base} <- {top}", matching
        // the overlay direction in code. Log lines depend on this
        // exact format.
        let base = NixpkgsBridge::new().with_label("stable").with_names(vec![]);
        let top = NixpkgsBridge::new().with_label("nightly").with_names(vec![]);
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));
        assert_eq!(overlay.label(), "stable <- nightly");
    }

    #[test]
    fn empty_overlay_yields_no_names_and_none_lookup() {
        // Genuinely empty closed universes on both sides. Lookup must
        // return None (not Err), and names() must be empty.
        let base = Empty { label: "b".into() };
        let top = Empty { label: "t".into() };
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));
        assert!(overlay.get("anything").unwrap().is_none());
        assert!(overlay.names().is_empty());
    }

    #[test]
    fn names_are_returned_sorted() {
        // Docstring says "Names enumerate the union"; the impl sorts
        // after merging. A future refactor that drops the sort call
        // would produce nondeterministic dashboards — pin the sort.
        let base = NixpkgsBridge::new()
            .with_names(vec!["zulu".into(), "alpha".into()])
            .with_label("base");
        let top = NixpkgsBridge::new()
            .with_names(vec!["mike".into(), "charlie".into()])
            .with_label("top");
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));
        let ns = overlay.names();
        assert_eq!(ns, vec!["alpha", "charlie", "mike", "zulu"]);
    }

    #[test]
    fn duplicate_names_across_sides_deduped() {
        // "bash" in both → appears once. Regression guard: if
        // `.contains` is dropped, dashboards list bash twice.
        let base = NixpkgsBridge::new()
            .with_names(vec!["bash".into(), "hello".into()])
            .with_label("base");
        let top = NixpkgsBridge::new()
            .with_names(vec!["bash".into(), "zsh".into()])
            .with_label("top");
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));
        let ns = overlay.names();
        let bash_count = ns.iter().filter(|n| *n == "bash").count();
        assert_eq!(bash_count, 1);
        assert_eq!(ns.len(), 3);
    }

    #[test]
    fn top_error_propagates() {
        // If `top.get` returns Err, the overlay surfaces it
        // immediately — it does NOT fall through to base.
        let base = NixpkgsBridge::new()
            .with_names(vec!["curl".into()])
            .with_label("base");
        let top = Erroring {
            reason: "top exploded".into(),
        };
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));
        let err = overlay.get("curl").unwrap_err();
        assert_eq!(err.to_string(), "backend: top exploded");
    }

    #[test]
    fn base_error_propagates_when_top_returns_none() {
        // `top.get` returns None (name not in closed universe), so we
        // fall through to base, which errors — that error must reach
        // the caller.
        let base = Erroring {
            reason: "base exploded".into(),
        };
        let top = NixpkgsBridge::new()
            .with_names(vec!["never".into()])
            .with_label("top");
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));
        let err = overlay.get("not-in-top").unwrap_err();
        assert_eq!(err.to_string(), "backend: base exploded");
    }

    #[test]
    fn only_base_resolves_when_top_universe_empty() {
        // Top is a closed universe with one name that won't match;
        // "hello" falls through to base.
        let base = NixpkgsBridge::new()
            .with_names(vec!["hello".into()])
            .with_label("base");
        let top = NixpkgsBridge::new()
            .with_names(vec!["nothing".into()])
            .with_label("top");
        let overlay = OverlayPackageSet::new(Box::new(base), Box::new(top));
        let hello = overlay.get("hello").unwrap().unwrap();
        assert_eq!(hello.name, "hello");
        // Came from base, so pkg_set is None (base had no custom expr).
        assert!(hello.bridge.unwrap().pkg_set.is_none());
    }
}
