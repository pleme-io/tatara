//! ANSI renderer — turns an `EventStream` into colored text with Nord styling.
//!
//! Honors `NO_COLOR`, auto-detects non-tty streams, and always writes
//! deterministic output (same events + same theme → identical bytes).

use std::io::Write;

use owo_colors::{OwoColorize, Style};

use crate::event::{ArtifactState, Cell, EventStream, LogLevel, UiEvent};
use crate::palette::{Rgb, Role, RoleMap};
use crate::sigil::Sigil;

/// Auto-detect whether to emit ANSI escapes.
/// `true` if stderr is a tty and `NO_COLOR` is not set.
pub fn should_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    // Crate's unix-only isatty probe. Stays conservative on non-Unix.
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = std::io::stderr().as_raw_fd();
        // SAFETY: isatty is a libc call on an owned fd.
        (unsafe { libc::isatty(fd) }) == 1
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub struct Renderer {
    pub role_map: RoleMap,
    pub color: bool,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new(RoleMap::default())
    }
}

impl Renderer {
    pub fn new(role_map: RoleMap) -> Self {
        Self {
            role_map,
            color: should_color(),
        }
    }

    pub fn plain(role_map: RoleMap) -> Self {
        Self {
            role_map,
            color: false,
        }
    }

    pub fn with_color(mut self, on: bool) -> Self {
        self.color = on;
        self
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
        let hash_s = self.text(&format!("blake3:{hash}"), Role::Dim);
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
        let hash = self.text(&format!("blake3:{root_hash}"), Role::Dim);
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
        let rgb = self.role_map.color_of(role);
        self.apply(s, rgb)
    }

    fn apply(&self, s: &str, rgb: Rgb) -> String {
        // owo-colors uses a 24-bit truecolor sequence — works in any
        // modern terminal (kitty, ghostty, iterm2, terminal.app 14+, tmux 3.2+).
        let style = Style::new().color(rgb.owo());
        s.style(style).to_string()
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
