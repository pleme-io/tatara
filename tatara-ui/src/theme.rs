//! `ThemeSpec` — the Lisp-authorable, content-addressable theme.

use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::palette::{Rgb, Role, RoleMap, NORD};

/// A theme is a named palette binding + sigil overrides + a BLAKE3-stable
/// identity. Themes compose (`extends`) and snapshot to disk.
///
/// Author in tatara-lisp:
///
/// ```lisp
/// (deftheme nord-arctic
///   :description "the canonical tatara look"
///   :semantic    (:info    "#81A1C1"
///                 :success "#A3BE8C"
///                 :warn    "#EBCB8B"
///                 :error   "#BF616A"
///                 :primary "#88C0D0"
///                 :accent  "#B48EAD"
///                 :dim     "#4C566A"))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[tatara(keyword = "deftheme")]
pub struct ThemeSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Name of a base theme to inherit from (e.g., `"nord-arctic"`). When set,
    /// unspecified roles fall back to the base. Resolution is the
    /// caller's responsibility via [`ThemeRegistry::resolve`].
    #[serde(default)]
    pub extends: Option<String>,
    /// Per-role hex strings. Any role not specified falls back to the base
    /// (if `extends` is set) or to `RoleMap::default()`.
    #[serde(default)]
    pub semantic: SemanticOverrides,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticOverrides {
    #[serde(default)]
    pub primary: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub info: Option<String>,
    #[serde(default)]
    pub success: Option<String>,
    #[serde(default)]
    pub warn: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub dim: Option<String>,
}

impl ThemeSpec {
    /// Built-in Nord-arctic theme — the default every tatara tool starts with.
    pub fn nord_arctic() -> Self {
        Self {
            name: "nord-arctic".into(),
            description: Some("the canonical tatara look — Nord palette, Aurora semantic roles".into()),
            extends: None,
            semantic: SemanticOverrides {
                primary: Some(NORD.nord8.as_hex()),
                accent: Some(NORD.nord15.as_hex()),
                info: Some(NORD.nord9.as_hex()),
                success: Some(NORD.nord14.as_hex()),
                warn: Some(NORD.nord13.as_hex()),
                error: Some(NORD.nord11.as_hex()),
                dim: Some(NORD.nord3.as_hex()),
            },
        }
    }

    /// Resolve the spec (no inheritance) into a concrete `RoleMap`. Any
    /// unspecified role falls back to `RoleMap::default()`.
    pub fn to_role_map(&self) -> RoleMap {
        let d = RoleMap::default();
        RoleMap {
            primary: self.semantic.primary.as_deref().and_then(parse_hex).unwrap_or(d.primary),
            accent: self.semantic.accent.as_deref().and_then(parse_hex).unwrap_or(d.accent),
            info: self.semantic.info.as_deref().and_then(parse_hex).unwrap_or(d.info),
            success: self.semantic.success.as_deref().and_then(parse_hex).unwrap_or(d.success),
            warn: self.semantic.warn.as_deref().and_then(parse_hex).unwrap_or(d.warn),
            error: self.semantic.error.as_deref().and_then(parse_hex).unwrap_or(d.error),
            dim: self.semantic.dim.as_deref().and_then(parse_hex).unwrap_or(d.dim),
        }
    }

    /// Content-addressable identity — BLAKE3 of the canonical JSON.
    /// Two specs with the same JSON produce the same id. Invariant across
    /// renderers, machines, runs.
    pub fn id(&self) -> ThemeId {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        ThemeId(hex::encode(blake3::hash(&bytes).as_bytes()))
    }
}

impl Default for ThemeSpec {
    fn default() -> Self {
        Self::nord_arctic()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThemeId(pub String);

impl ThemeId {
    pub fn short(&self) -> &str {
        &self.0[..16.min(self.0.len())]
    }
}

impl std::fmt::Display for ThemeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "theme:{}", self.short())
    }
}

/// Parse `"#RRGGBB"` (case-insensitive) into an `Rgb`.
fn parse_hex(s: &str) -> Option<Rgb> {
    let t = s.trim();
    let t = t.strip_prefix('#').unwrap_or(t);
    if t.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let g = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    Some(Rgb(r, g, b))
}

/// Tiny registry — themes in memory, resolves `extends` chains.
#[derive(Default)]
pub struct ThemeRegistry {
    themes: std::collections::BTreeMap<String, ThemeSpec>,
}

impl ThemeRegistry {
    pub fn new() -> Self {
        let mut r = Self::default();
        r.register(ThemeSpec::nord_arctic());
        r
    }

    pub fn register(&mut self, spec: ThemeSpec) {
        self.themes.insert(spec.name.clone(), spec);
    }

    pub fn get(&self, name: &str) -> Option<&ThemeSpec> {
        self.themes.get(name)
    }

    /// Resolve `extends` — merge child overrides on top of the parent's role map.
    pub fn resolve(&self, name: &str) -> Option<RoleMap> {
        let spec = self.themes.get(name)?;
        let base = match spec.extends.as_deref() {
            Some(parent) => self.resolve(parent).unwrap_or_default(),
            None => RoleMap::default(),
        };
        let overrides = spec.to_role_map();
        Some(RoleMap {
            primary: spec.semantic.primary.as_ref().map_or(base.primary, |_| overrides.primary),
            accent: spec.semantic.accent.as_ref().map_or(base.accent, |_| overrides.accent),
            info: spec.semantic.info.as_ref().map_or(base.info, |_| overrides.info),
            success: spec.semantic.success.as_ref().map_or(base.success, |_| overrides.success),
            warn: spec.semantic.warn.as_ref().map_or(base.warn, |_| overrides.warn),
            error: spec.semantic.error.as_ref().map_or(base.error, |_| overrides.error),
            dim: spec.semantic.dim.as_ref().map_or(base.dim, |_| overrides.dim),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn nord_arctic_resolves_to_nord_defaults() {
        let rm = ThemeSpec::nord_arctic().to_role_map();
        assert_eq!(rm.primary.as_hex(), "#88C0D0");
        assert_eq!(rm.success.as_hex(), "#A3BE8C");
    }

    #[test]
    fn id_is_content_addressed_and_stable() {
        let a = ThemeSpec::nord_arctic();
        let b = ThemeSpec::nord_arctic();
        assert_eq!(a.id(), b.id());
        assert_eq!(a.id().0.len(), 64); // BLAKE3 hex
    }

    #[test]
    fn id_changes_when_any_role_flips() {
        let mut a = ThemeSpec::nord_arctic();
        a.semantic.accent = Some("#D08770".into()); // orange, not purple
        assert_ne!(a.id(), ThemeSpec::nord_arctic().id());
    }

    #[test]
    fn lisp_round_trip() {
        let src = r##"(deftheme
          :name        "warm"
          :description "swap purple accent for orange"
          :extends     "nord-arctic"
          :semantic    (:accent "#D08770"))"##;
        let forms = read(src).unwrap();
        let t = ThemeSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(t.name, "warm");
        assert_eq!(t.extends.as_deref(), Some("nord-arctic"));
        assert_eq!(t.semantic.accent.as_deref(), Some("#D08770"));
    }

    #[test]
    fn registry_resolves_extends_chain() {
        let mut reg = ThemeRegistry::new();
        reg.register(ThemeSpec {
            name: "warm".into(),
            description: None,
            extends: Some("nord-arctic".into()),
            semantic: SemanticOverrides {
                accent: Some("#D08770".into()),
                ..Default::default()
            },
        });
        let rm = reg.resolve("warm").unwrap();
        // accent overridden
        assert_eq!(rm.accent.as_hex(), "#D08770");
        // primary inherits from nord-arctic
        assert_eq!(rm.primary.as_hex(), "#88C0D0");
    }

    #[test]
    fn parse_hex_accepts_with_and_without_prefix() {
        assert_eq!(parse_hex("#88C0D0"), Some(Rgb(0x88, 0xC0, 0xD0)));
        assert_eq!(parse_hex("88c0d0"), Some(Rgb(0x88, 0xC0, 0xD0)));
        assert_eq!(parse_hex("not-hex"), None);
    }
}
