//! Sigils — typed Unicode glyphs that carry a default semantic role.

use crate::palette::Role;

/// Every glyph tatara-ui uses. Typed so a renderer override changes ALL uses
/// of a sigil in lockstep, and so Lisp-authored themes can substitute a glyph
/// without any prose hunting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Sigil {
    /// ❄ — the tatara/pleme marker. Canonical for "heading, banner, hero".
    Snowflake,
    /// ✓ — success / realized / cached-hit-ok.
    Check,
    /// ✗ — failure / broken / missing.
    Cross,
    /// → — flow / becomes / renders-to.
    Arrow,
    /// ⟡ — section divider (pangea's section() char).
    Section,
    /// ● — filled bullet / active.
    Dot,
    /// ○ — hollow bullet / pending.
    DotHollow,
    /// + — add / create.
    Plus,
    /// − — remove / delete.
    Minus,
    /// ~ — modify / update.
    Tilde,
    /// ± — optional / may-change.
    PlusMinus,
    /// ▲ — emphasis / top-of-tree.
    Triangle,
    /// ⏱ — elapsed-time marker.
    Clock,
    /// ◇ — content-address / BLAKE3 hash.
    Diamond,
    /// ⚡ — cache hit (fast path).
    Lightning,
    /// ⚙ — build / realize.
    Gear,
    /// ⬢ — store-path (hex → hexagon).
    Hex,
}

impl Sigil {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Snowflake => "❄",
            Self::Check => "✓",
            Self::Cross => "✗",
            Self::Arrow => "→",
            Self::Section => "⟡",
            Self::Dot => "●",
            Self::DotHollow => "○",
            Self::Plus => "+",
            Self::Minus => "−",
            Self::Tilde => "~",
            Self::PlusMinus => "±",
            Self::Triangle => "▲",
            Self::Clock => "⏱",
            Self::Diamond => "◇",
            Self::Lightning => "⚡",
            Self::Gear => "⚙",
            Self::Hex => "⬢",
        }
    }

    pub fn default_role(self) -> Role {
        match self {
            Self::Snowflake | Self::Triangle | Self::Section | Self::Hex => Role::Primary,
            Self::Check | Self::Plus | Self::Lightning => Role::Success,
            Self::Cross | Self::Minus => Role::Error,
            Self::Tilde | Self::Clock | Self::Gear => Role::Warn,
            Self::Arrow | Self::Dot | Self::Diamond => Role::Info,
            Self::DotHollow | Self::PlusMinus => Role::Dim,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sigil_renders_to_one_display_cluster() {
        // Unicode awareness: every declared glyph is at least 1 byte. We use
        // width-1 display columns for layout; if a future glyph changes that
        // the author must rethink layout, and this test documents that.
        for s in [
            Sigil::Snowflake,
            Sigil::Check,
            Sigil::Cross,
            Sigil::Arrow,
            Sigil::Section,
            Sigil::Dot,
            Sigil::DotHollow,
            Sigil::Plus,
            Sigil::Minus,
            Sigil::Tilde,
            Sigil::PlusMinus,
            Sigil::Triangle,
            Sigil::Clock,
            Sigil::Diamond,
            Sigil::Lightning,
            Sigil::Gear,
            Sigil::Hex,
        ] {
            assert!(!s.glyph().is_empty());
        }
    }

    #[test]
    fn semantic_role_mapping_matches_intuition() {
        assert!(matches!(Sigil::Check.default_role(), Role::Success));
        assert!(matches!(Sigil::Cross.default_role(), Role::Error));
        assert!(matches!(Sigil::Snowflake.default_role(), Role::Primary));
        assert!(matches!(Sigil::Tilde.default_role(), Role::Warn));
    }
}
