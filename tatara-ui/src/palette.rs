//! Nord palette — canonical hex values from <https://www.nordtheme.com>,
//! matched to pleme-io's starship + irodori conventions.

/// A 24-bit RGB color — the wire form for all tatara-ui palette values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn from_hex(rgb: u32) -> Self {
        Self(
            ((rgb >> 16) & 0xff) as u8,
            ((rgb >> 8) & 0xff) as u8,
            (rgb & 0xff) as u8,
        )
    }

    /// `owo_colors::Rgb` conversion — drops us into the 24-bit ANSI ecosystem.
    pub fn owo(self) -> owo_colors::Rgb {
        owo_colors::Rgb(self.0, self.1, self.2)
    }

    pub fn as_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

/// The 16-color Nord palette. Named by the upstream conventions.
pub struct NordPalette {
    // Polar Night (darks → dims)
    pub nord0: Rgb,
    pub nord1: Rgb,
    pub nord2: Rgb,
    pub nord3: Rgb,
    // Snow Storm (neutrals, foreground)
    pub nord4: Rgb,
    pub nord5: Rgb,
    pub nord6: Rgb,
    // Frost (cool accents — primary + info + progress)
    pub nord7: Rgb,
    pub nord8: Rgb,
    pub nord9: Rgb,
    pub nord10: Rgb,
    // Aurora (warm accents — semantic roles)
    pub nord11: Rgb,
    pub nord12: Rgb,
    pub nord13: Rgb,
    pub nord14: Rgb,
    pub nord15: Rgb,
}

pub const NORD: NordPalette = NordPalette {
    nord0: Rgb::from_hex(0x2E3440),
    nord1: Rgb::from_hex(0x3B4252),
    nord2: Rgb::from_hex(0x434C5E),
    nord3: Rgb::from_hex(0x4C566A),
    nord4: Rgb::from_hex(0xD8DEE9),
    nord5: Rgb::from_hex(0xE5E9F0),
    nord6: Rgb::from_hex(0xECEFF4),
    nord7: Rgb::from_hex(0x8FBCBB),
    nord8: Rgb::from_hex(0x88C0D0),
    nord9: Rgb::from_hex(0x81A1C1),
    nord10: Rgb::from_hex(0x5E81AC),
    nord11: Rgb::from_hex(0xBF616A),
    nord12: Rgb::from_hex(0xD08770),
    nord13: Rgb::from_hex(0xEBCB8B),
    nord14: Rgb::from_hex(0xA3BE8C),
    nord15: Rgb::from_hex(0xB48EAD),
};

/// Seven semantic roles — the public vocabulary tooling renders against.
/// Matches pangea-core/theme.rb's CLI role vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Role {
    Primary,
    Accent,
    Info,
    Success,
    Warn,
    Error,
    Dim,
}

/// Default Nord → Role mapping. Every tatara-ui consumer starts with this
/// and may override via `ThemeSpec::semantic`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RoleMap {
    pub primary: Rgb,
    pub accent: Rgb,
    pub info: Rgb,
    pub success: Rgb,
    pub warn: Rgb,
    pub error: Rgb,
    pub dim: Rgb,
}

impl Default for RoleMap {
    fn default() -> Self {
        Self {
            primary: NORD.nord8,  // frost cyan
            accent: NORD.nord15,  // aurora purple
            info: NORD.nord9,     // frost blue
            success: NORD.nord14, // aurora green
            warn: NORD.nord13,    // aurora yellow
            error: NORD.nord11,   // aurora red
            dim: NORD.nord3,      // polar night lightest
        }
    }
}

impl RoleMap {
    pub fn color_of(&self, role: Role) -> Rgb {
        match role {
            Role::Primary => self.primary,
            Role::Accent => self.accent,
            Role::Info => self.info,
            Role::Success => self.success,
            Role::Warn => self.warn,
            Role::Error => self.error,
            Role::Dim => self.dim,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nord_canonical_hex_values() {
        assert_eq!(NORD.nord8.as_hex(), "#88C0D0"); // cyan / primary
        assert_eq!(NORD.nord11.as_hex(), "#BF616A"); // red / error
        assert_eq!(NORD.nord14.as_hex(), "#A3BE8C"); // green / success
        assert_eq!(NORD.nord13.as_hex(), "#EBCB8B"); // yellow / warn
    }

    #[test]
    fn default_rolemap_binds_semantic_to_nord() {
        let rm = RoleMap::default();
        assert_eq!(rm.success.as_hex(), "#A3BE8C");
        assert_eq!(rm.error.as_hex(), "#BF616A");
        assert_eq!(rm.warn.as_hex(), "#EBCB8B");
    }

    #[test]
    fn rgb_owo_roundtrip_preserves_bytes() {
        let c = NORD.nord8;
        let o = c.owo();
        assert_eq!((o.0, o.1, o.2), (c.0, c.1, c.2));
    }
}
