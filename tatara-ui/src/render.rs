//! ANSI renderer — turns an `EventStream` into colored text with Nord styling.
//!
//! Emission goes through kazari: its capability wall decides the color depth
//! (truecolor → 256 → 16 → none on pipes, `NO_COLOR`, dumb terminals), and its
//! typed SGR atom writes every escape byte. Same events + same theme + same
//! capability → identical bytes.

use std::io::Write;

use kazari::{Capability, ColorLevel, Stream};

use crate::event::{ArtifactState, Cell, EventStream, LogLevel, UiEvent};
use crate::palette::{Rgb, Role, RoleMap};
use crate::sigil::Sigil;

/// Auto-detect whether to emit ANSI escapes on stderr, through kazari's
/// capability probe (`CLICOLOR_FORCE` > `NO_COLOR` > tty > `COLORTERM` > `TERM`).
pub fn should_color() -> bool {
    Capability::probe_stream(Stream::Stderr).level.is_colored()
}

pub struct Renderer {
    pub role_map: RoleMap,
    pub color: bool,
    pub caps: Capability,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new(RoleMap::default())
    }
}

impl Renderer {
    pub fn new(role_map: RoleMap) -> Self {
        let caps = Capability::probe_stream(Stream::Stderr);
        Self {
            role_map,
            color: caps.level.is_colored(),
            caps,
        }
    }

    pub fn plain(role_map: RoleMap) -> Self {
        Self {
            role_map,
            color: false,
            caps: Capability::plain(),
        }
    }

    pub fn with_color(mut self, on: bool) -> Self {
        self.color = on;
        self
    }

    pub fn with_caps(mut self, caps: Capability) -> Self {
        self.color = caps.level.is_colored();
        self.caps = caps;
        self
    }

    fn emit_caps(&self) -> Capability {
        if self.caps.level.is_colored() {
            self.caps
        } else {
            Capability {
                level: ColorLevel::Truecolor,
                ..self.caps
            }
        }
    }

    pub fn render(&self, events: &EventStream, w: &mut impl Write) -> std::io::Result<()> {
        for e in &events.events {
            self.render_one(e, w)?;
        }
        Ok(())
    }

    pub fn render_one(&self, e: &UiEvent, w: &mut impl Write) -> std::io::Result<()> {
        match e {
            UiEvent::Banner { title, subtitle } => self.banner(title, subtitle.as_deref(), w),
            UiEvent::Section { title } => self.section(title, w),
            UiEvent::Log { level, message } => self.log(*level, message, w),
            UiEvent::PhaseBegin { phase } => {
                let arrow = self.glyph(Sigil::Arrow);
                let phase_s = self.text(phase, Role::Primary);
                writeln!(w, "  {arrow} {phase_s}")
            }
            UiEvent::PhaseEnd { phase, elapsed_ms } => {
                let check = self.glyph(Sigil::Check);
                let phase_s = self.text(phase, Role::Dim);
                let elapsed = self.dim_elapsed(*elapsed_ms);
                writeln!(w, "  {check} {phase_s} {elapsed}")
            }
            UiEvent::Artifact { name, hash, state } => self.artifact(name, hash, state, w),
            UiEvent::Summary {
                root_hash,
                total,
                built,
                cached,
                failed,
            } => self.summary(root_hash, *total, *built, *cached, *failed, w),
            UiEvent::Row { cells } => self.row(cells, w),
        }
    }

    fn banner(
        &self,
        title: &str,
        subtitle: Option<&str>,
        w: &mut impl Write,
    ) -> std::io::Result<()> {
        let snow = self.glyph(Sigil::Snowflake);
        let title_s = self.text(title, Role::Primary);
        writeln!(w, "{snow} {title_s}")?;
        if let Some(sub) = subtitle {
            let sub_s = self.text(sub, Role::Dim);
            writeln!(w, "  {sub_s}")?;
        }
        Ok(())
    }

    fn section(&self, title: &str, w: &mut impl Write) -> std::io::Result<()> {
        let sec = self.glyph(Sigil::Section);
        let title_s = self.text(title, Role::Primary);
        writeln!(w)?;
        writeln!(w, "{sec} {title_s}")?;
        // Subtle Nord-dim underline
        let line_char = "─";
        let rule = line_char.repeat(2 + title.chars().count() + 1);
        let rule_s = self.text(&rule, Role::Dim);
        writeln!(w, "{rule_s}")?;
        Ok(())
    }

    fn log(&self, level: LogLevel, message: &str, w: &mut impl Write) -> std::io::Result<()> {
        let sigil = match level {
            LogLevel::Success => Sigil::Check,
            LogLevel::Error => Sigil::Cross,
            LogLevel::Warn => Sigil::Tilde,
            LogLevel::Info => Sigil::Dot,
            LogLevel::Dim => Sigil::DotHollow,
        };
        let s = self.colored_glyph(sigil, level.role());
        let msg = self.text(message, level.role());
        writeln!(w, "  {s} {msg}")
    }

    fn artifact(
        &self,
        name: &str,
        hash: &crate::event::ShortHash,
        state: &ArtifactState,
        w: &mut impl Write,
    ) -> std::io::Result<()> {
        // ❄ <name>  ◇blake3:cxx3i50  ⚡ cached | ⚙ built 5.3s | ○ pending | ✗ failed
        let snow = self.colored_glyph(Sigil::Snowflake, Role::Primary);
        let name_s = self.text(name, Role::Primary);
        let diamond = self.colored_glyph(Sigil::Diamond, Role::Info);
        let hash_s = self.text(&crate::hash::blake3_scheme_display(hash), Role::Dim);
        let state_chunk = self.state_chunk(state);
        writeln!(w, "  {snow} {name_s:<28} {diamond} {hash_s}  {state_chunk}")
    }

    fn state_chunk(&self, state: &ArtifactState) -> String {
        match state {
            ArtifactState::Built { elapsed_ms } => {
                let g = self.colored_glyph(Sigil::Gear, Role::Warn);
                let label = self.text("built", Role::Success);
                let ms = self.text(&format!("{:.1}s", *elapsed_ms as f64 / 1000.0), Role::Dim);
                format!("{g} {label} {ms}")
            }
            ArtifactState::Cached => {
                let g = self.colored_glyph(Sigil::Lightning, Role::Success);
                let label = self.text("cached", Role::Success);
                format!("{g} {label}")
            }
            ArtifactState::Pending => {
                let g = self.colored_glyph(Sigil::DotHollow, Role::Dim);
                let label = self.text("pending", Role::Dim);
                format!("{g} {label}")
            }
            ArtifactState::Failed { reason } => {
                let g = self.colored_glyph(Sigil::Cross, Role::Error);
                let label = self.text("failed", Role::Error);
                let reason_s = self.text(reason, Role::Error);
                format!("{g} {label} {reason_s}")
            }
        }
    }

    fn summary(
        &self,
        root_hash: &crate::event::ShortHash,
        total: usize,
        built: usize,
        cached: usize,
        failed: usize,
        w: &mut impl Write,
    ) -> std::io::Result<()> {
        writeln!(w)?;
        let tri = self.colored_glyph(Sigil::Triangle, Role::Primary);
        let label = self.text("content root", Role::Primary);
        let diamond = self.colored_glyph(Sigil::Diamond, Role::Info);
        let hash = self.text(&crate::hash::blake3_scheme_display(root_hash), Role::Dim);
        writeln!(w, "{tri} {label}  {diamond} {hash}")?;
        let summary =
            format!("  {total} total · {built} built · {cached} cached · {failed} failed",);
        let role = if failed > 0 {
            Role::Error
        } else {
            Role::Success
        };
        writeln!(w, "{}", self.text(&summary, role))?;
        Ok(())
    }

    fn row(&self, cells: &[Cell], w: &mut impl Write) -> std::io::Result<()> {
        let mut parts = Vec::with_capacity(cells.len());
        for c in cells {
            let role = c.role.unwrap_or(Role::Info);
            parts.push(self.text(&c.text, role));
        }
        writeln!(w, "  {}", parts.join("  "))
    }

    // ── primitives ──────────────────────────────────────────────────────

    fn glyph(&self, s: Sigil) -> String {
        self.colored_glyph(s, s.default_role())
    }

    fn colored_glyph(&self, s: Sigil, r: Role) -> String {
        self.text(s.glyph(), r)
    }

    fn text(&self, s: &str, role: Role) -> String {
        if !self.color {
            return s.to_string();
        }
        self.apply(s, self.role_map.color_of(role))
    }

    fn apply(&self, s: &str, rgb: Rgb) -> String {
        kazari::paint_rgb_at(rgb.to_kazari(), s, false, false, &self.emit_caps())
    }

    fn dim_elapsed(&self, ms: u64) -> String {
        self.text(&format!("{:.1}s", ms as f64 / 1000.0), Role::Dim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ShortHash;

    fn plain() -> Renderer {
        Renderer::plain(RoleMap::default())
    }

    #[test]
    fn banner_with_no_color_contains_plain_snowflake_and_title() {
        let r = plain();
        let mut out: Vec<u8> = Vec::new();
        r.render_one(
            &UiEvent::Banner {
                title: "tatara-boot-gen".into(),
                subtitle: Some("plex".into()),
            },
            &mut out,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains('❄'));
        assert!(s.contains("tatara-boot-gen"));
        assert!(s.contains("plex"));
    }

    #[test]
    fn artifact_renders_name_hash_state() {
        let r = plain();
        let mut out: Vec<u8> = Vec::new();
        r.render_one(
            &UiEvent::Artifact {
                name: "initrd-plex".into(),
                hash: ShortHash::from_blake3_hex("cxx3i50l"),
                state: ArtifactState::Cached,
            },
            &mut out,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("initrd-plex"));
        assert!(s.contains("blake3:cxx3i50"));
        assert!(s.contains("cached"));
    }

    #[test]
    fn summary_shows_totals_and_hash() {
        let r = plain();
        let mut out: Vec<u8> = Vec::new();
        r.render_one(
            &UiEvent::Summary {
                root_hash: ShortHash::from_blake3_hex("abcd1234"),
                total: 7,
                built: 3,
                cached: 4,
                failed: 0,
            },
            &mut out,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("blake3:abcd123"));
        assert!(s.contains("7 total"));
        assert!(s.contains("3 built"));
        assert!(s.contains("4 cached"));
    }

    #[test]
    fn section_renders_divider_rule() {
        let r = plain();
        let mut out: Vec<u8> = Vec::new();
        r.render_one(
            &UiEvent::Section {
                title: "synthesize".into(),
            },
            &mut out,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains('⟡'));
        assert!(s.contains("synthesize"));
        assert!(s.contains('─'));
    }

    fn render_log(r: &Renderer) -> String {
        let mut out: Vec<u8> = Vec::new();
        r.render_one(
            &UiEvent::Log {
                level: LogLevel::Info,
                message: "m".into(),
            },
            &mut out,
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn plain_capability_emits_zero_escape_bytes() {
        let r = Renderer::new(RoleMap::default()).with_caps(Capability::plain());
        assert!(!r.color);
        assert!(!render_log(&r).contains('\u{1b}'));
    }

    #[test]
    fn truecolor_emits_the_role_rgb() {
        let caps = Capability::fixed(ColorLevel::Truecolor, 80, true);
        let r = Renderer::plain(RoleMap::default()).with_caps(caps);
        let s = render_log(&r);
        let info = RoleMap::default().info;
        assert!(
            s.contains(&format!("38;2;{};{};{}", info.0, info.1, info.2)),
            "{s:?}"
        );
    }

    #[test]
    fn ansi256_degrades_through_kazari() {
        let caps = Capability::fixed(ColorLevel::Ansi256, 80, true);
        let r = Renderer::plain(RoleMap::default()).with_caps(caps);
        let s = render_log(&r);
        assert!(s.contains("38;5;"), "{s:?}");
        assert!(!s.contains("38;2;"), "{s:?}");
    }

    #[test]
    fn forced_color_on_a_pipe_stays_truecolor() {
        let r = Renderer::plain(RoleMap::default()).with_color(true);
        assert!(render_log(&r).contains("38;2;"));
    }

    #[test]
    fn theme_override_rgb_reaches_the_bytes() {
        let rm = RoleMap {
            info: Rgb(0x12, 0x34, 0x56),
            ..RoleMap::default()
        };
        let caps = Capability::fixed(ColorLevel::Truecolor, 80, true);
        let r = Renderer::plain(rm).with_caps(caps);
        assert!(render_log(&r).contains("38;2;18;52;86"));
    }

    #[test]
    fn log_sigils_match_level() {
        let r = plain();
        for (level, expected_glyph) in [
            (LogLevel::Success, '✓'),
            (LogLevel::Error, '✗'),
            (LogLevel::Warn, '~'),
        ] {
            let mut out: Vec<u8> = Vec::new();
            r.render_one(
                &UiEvent::Log {
                    level,
                    message: "m".into(),
                },
                &mut out,
            )
            .unwrap();
            let s = String::from_utf8(out).unwrap();
            assert!(
                s.contains(expected_glyph),
                "{level:?} should use {expected_glyph}"
            );
        }
    }
}
