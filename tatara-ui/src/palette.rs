//! Nord palette — sourced from `irodori::NORD` and resolved per role through
//! kazari's dark-Nord theme, the fleet's single line-output styling system.

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

    pub const fn from_irodori(c: irodori::Color) -> Self {
        Self(c.r, c.g, c.b)
    }

    pub const fn to_kazari(self) -> kazari::Rgb {
        kazari::Rgb::new(self.0, self.1, self.2)
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

pub const NORD: NordPalette = {
    let n = irodori::NORD;
    NordPalette {
        nord0: Rgb::from_irodori(n.polar_night[0]),
        nord1: Rgb::from_irodori(n.polar_night[1]),
        nord2: Rgb::from_irodori(n.polar_night[2]),
        nord3: Rgb::from_irodori(n.polar_night[3]),
        nord4: Rgb::from_irodori(n.snow_storm[0]),
        nord5: Rgb::from_irodori(n.snow_storm[1]),
        nord6: Rgb::from_irodori(n.snow_storm[2]),
        nord7: Rgb::from_irodori(n.frost[0]),
        nord8: Rgb::from_irodori(n.frost[1]),
        nord9: Rgb::from_irodori(n.frost[2]),
        nord10: Rgb::from_irodori(n.frost[3]),
        nord11: Rgb::from_irodori(n.aurora[0]),
        nord12: Rgb::from_irodori(n.aurora[1]),
        nord13: Rgb::from_irodori(n.aurora[2]),
        nord14: Rgb::from_irodori(n.aurora[3]),
        nord15: Rgb::from_irodori(n.aurora[4]),
    }
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

impl Role {
    pub const ALL: [Role; 7] = [
        Role::Primary,
        Role::Accent,
        Role::Info,
        Role::Success,
        Role::Warn,
        Role::Error,
        Role::Dim,
    ];

    pub const fn kazari(self) -> kazari::Role {
        match self {
            Role::Primary => kazari::Role::Primary,
            Role::Accent => kazari::Role::Ident,
            Role::Info => kazari::Role::Info,
            Role::Success => kazari::Role::Ok,
            Role::Warn => kazari::Role::Pending,
            Role::Error => kazari::Role::Error,
            Role::Dim => kazari::Role::TextDim,
        }
    }
}

impl From<Rgb> for kazari::Rgb {
    fn from(c: Rgb) -> Self {
        c.to_kazari()
    }
}

impl From<kazari::Rgb> for Rgb {
    fn from(c: kazari::Rgb) -> Self {
        Self(c.r, c.g, c.b)
    }
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
        let resolve = |role: Role| Rgb::from(kazari::Theme::default().color(role.kazari()));
        Self {
            primary: resolve(Role::Primary),
            accent: resolve(Role::Accent),
            info: resolve(Role::Info),
            success: resolve(Role::Success),
            warn: resolve(Role::Warn),
            error: resolve(Role::Error),
            dim: resolve(Role::Dim),
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
    fn rgb_kazari_roundtrip_preserves_bytes() {
        let c = NORD.nord8;
        let k = c.to_kazari();
        assert_eq!((k.r, k.g, k.b), (c.0, c.1, c.2));
        assert_eq!(Rgb::from(k), c);
    }

    #[test]
    fn nord_is_irodori_nord() {
        let ours: Vec<Rgb> = [
            NORD.nord0,
            NORD.nord1,
            NORD.nord2,
            NORD.nord3,
            NORD.nord4,
            NORD.nord5,
            NORD.nord6,
            NORD.nord7,
            NORD.nord8,
            NORD.nord9,
            NORD.nord10,
            NORD.nord11,
            NORD.nord12,
            NORD.nord13,
            NORD.nord14,
            NORD.nord15,
        ]
        .to_vec();
        let theirs: Vec<Rgb> = irodori::NORD.iter().map(Rgb::from_irodori).collect();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn default_rolemap_is_kazari_theme() {
        let rm = RoleMap::default();
        for role in Role::ALL {
            let k = kazari::Theme::default().color(role.kazari());
            assert_eq!(rm.color_of(role), Rgb::from(k), "{role:?}");
        }
    }

    #[test]
    fn default_rolemap_keeps_its_nord_slots() {
        let rm = RoleMap::default();
        assert_eq!(rm.primary, NORD.nord8);
        assert_eq!(rm.accent, NORD.nord15);
        assert_eq!(rm.info, NORD.nord9);
        assert_eq!(rm.success, NORD.nord14);
        assert_eq!(rm.warn, NORD.nord13);
        assert_eq!(rm.error, NORD.nord11);
        assert_eq!(rm.dim, NORD.nord3);
    }
}
