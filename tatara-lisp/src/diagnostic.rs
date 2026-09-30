//! Source-position rendering for `LispError`.
//!
//! `tatara-lisp` errors carry byte offsets through the reader (see
//! `reader.rs` and `LispError::position`). This module is the projection
//! step: it converts a byte offset into a 1-based `(line, column)` and
//! renders a rustc-style diagnostic with a caret pointing at the
//! failure. `tatara-lispc`, `tatara-check`, the REPL, and the future
//! LSP all funnel through `format_diagnostic` so authoring surfaces
//! point at the byte that broke instead of leaving the operator to
//! hunt for it.
//!
//! Theory grounding: THEORY.md §V.1 — knowable platform / constructive
//! diagnostics. An error whose location cannot be projected to source
//! is not knowable. Inspiration: rustc's `DiagnosticBuilder` snippet
//! format; translation through pleme-io primitives is byte-offset
//! spans on the existing `LispError`, no new IR layer.

use crate::error::LispError;

/// `writeln!` into a writer whose `fmt::Write` impl is infallible.
///
/// [`SourceProjection::render_snippet_body`] assembles the rustc-style
/// snippet by emitting four formatted lines into a `String`, and
/// `format_diagnostic` composes on top of it. `String`'s `fmt::Write`
/// impl is total — `impl fmt::Write for String { fn write_str(&mut
/// self, s) { self.push_str(s); Ok(()) } }` — so every
/// `writeln!`/`write!` into it returns `Ok(())`; the inline
/// `.expect("writes to a String never fail")` triple recurred at four
/// sites (THEORY.md §VI.1 three-times rule, crossed decisively).
///
/// Lifting it into ONE macro centralizes the canonical panic message:
/// a typo in the expect-string can never drift across the four
/// emission sites at runtime. Sibling of `infallible_write!` for the
/// non-newline-terminated single-write case (the trailing caret line
/// in `format_diagnostic`).
///
/// Theory grounding: THEORY.md §VI.1 — the four-times duplication of
/// `.expect("writes to a String never fail")` collapses into one
/// named primitive. The macro names the invariant ("infallible write
/// to String") as a primitive of the diagnostic-rendering substrate,
/// so future writer-type changes (e.g., `String` → a typed builder)
/// land in ONE place — every call site picks up the new emission
/// posture mechanically.
macro_rules! infallible_writeln {
    ($out:expr, $($t:tt)*) => {{
        // Hygienically bring `fmt::Write::write_fmt` into scope so
        // call sites don't need a separate `use std::fmt::Write as _`
        // import — the macro is self-contained.
        use ::std::fmt::Write as _;
        ::std::writeln!($out, $($t)*).expect("writes to a String never fail")
    }};
}

/// `write!` into a writer whose `fmt::Write` impl is infallible — the
/// non-newline-terminated sibling of `infallible_writeln!`. Used by
/// [`SourceProjection::render_snippet_body`] for its trailing caret
/// line, which must not emit a closing newline (so consumers
/// concatenating the rendered diagnostic into a longer message see the
/// caret as the final character, not the line after it). Same
/// `String`-infallibility invariant; same canonical panic message;
/// same theory-anchor (THEORY.md §VI.1).
macro_rules! infallible_write {
    ($out:expr, $($t:tt)*) => {{
        use ::std::fmt::Write as _;
        ::std::write!($out, $($t)*).expect("writes to a String never fail")
    }};
}

/// 1-based line + column. `line_col` walks the source up to a byte
/// offset; `\n` increments `line` and resets `column` to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
}

/// Projection of a byte offset into a source string onto the four
/// rendered components a diagnostic renderer needs to place a caret
/// under the offending byte on a fixed-width terminal: 1-based line,
/// 1-based column (UTF-8 scalars, not bytes), the containing source
/// line, and the tab-mirrored caret pad whose rendered width equals
/// the first `column - 1` chars of the source line.
///
/// [`format_diagnostic`] composes on top of this projection — every
/// consumer that renders diagnostics into its own writer (an LSP
/// surfacing structured `Diagnostic` records, an IDE painting a
/// custom underline, a `tatara-check` variant routing to a JSON
/// emitter instead of a rustc-style snippet) reaches the four typed
/// components in ONE call rather than composing three inline helpers,
/// and the tab-mirror discipline that keeps carets aligned under
/// tab-indented sources is inherited from the substrate automatically.
///
/// Pre-lift the four components were computed inline at
/// [`format_diagnostic`]'s body via three sibling helpers ([`line_col`],
/// a private `line_at`, a private `mirror_source_prefix_as_pad`).
/// Consumers wanting the same projection outside `format_diagnostic`
/// had no first-class named projection to reach for — they either
/// re-implemented `line_at` + `mirror_source_prefix_as_pad` (silently
/// drifting the tab-mirror discipline) or called `format_diagnostic`
/// and substring-parsed its rendered snippet. This struct + its
/// [`Self::at`] constructor is the ONE named projection every renderer
/// binds to; a regression in the substrate's tab-mirror discipline,
/// column-counting semantics (chars vs bytes), or EOF-clamp corner
/// reaches every consumer through ONE edit.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / constructive
/// diagnostics. The byte-offset → rendered-components projection is
/// exposed as ONE typed value so downstream renderers cannot silently
/// drift the tab-mirror discipline or the 1-based origin from the
/// substrate. THEORY.md §VI.1 — generation over composition; the
/// three-helper composition inside `format_diagnostic` is named as
/// ONE typed projection every future emitter binds to rather than
/// re-derives.
#[derive(Debug, Clone)]
pub struct SourceProjection<'a> {
    /// 1-based line number of the line containing `byte_offset`.
    pub line: usize,
    /// 1-based column (UTF-8 scalars, not bytes) of `byte_offset`
    /// within the containing line.
    pub column: usize,
    /// The containing source line, without its trailing `\n`.
    pub line_text: &'a str,
    /// A pad whose rendered visual width on a fixed-width terminal
    /// equals the first `column - 1` chars of [`Self::line_text`].
    /// Each source `\t` mirrors through as `\t`; every other char
    /// becomes a space. The pad and the source line consume the same
    /// tab-stops, so a caret placed after the pad lands under the
    /// offending byte regardless of the terminal's tab-stop setting.
    pub caret_pad: String,
}

impl<'a> SourceProjection<'a> {
    /// Project `byte_offset` into `src` onto the four rendered
    /// components. Offsets past EOF clamp to the final position; the
    /// column counts UTF-8 scalar characters, not bytes (an `é` is
    /// one column, two bytes) so the caret renders under the visible
    /// character a human sees.
    ///
    /// Composes [`line_col`] with the containing-line slice
    /// (`line_at`) and the tab-mirrored caret pad
    /// (`mirror_source_prefix_as_pad`) at ONE call site. A regression
    /// that swapped ONE component's implementation (e.g. sliced
    /// [`Self::line_text`] by byte offset rather than char offset,
    /// or padded the caret with spaces rather than mirroring tabs)
    /// would silently drift the caret placement on tab-indented
    /// sources — the sibling `format_diagnostic_caret_pad_mirrors_*`
    /// tests below AND the new `SourceProjection`-level tests both
    /// bind that invariant at the substrate boundary.
    #[must_use]
    pub fn at(src: &'a str, byte_offset: usize) -> Self {
        let LineCol { line, column } = line_col(src, byte_offset);
        let line_text = line_at(src, byte_offset);
        let caret_pad = mirror_source_prefix_as_pad(line_text, column);
        Self {
            line,
            column,
            line_text,
            caret_pad,
        }
    }

    /// Render the rustc-style snippet body — the location line, the
    /// gutter, the source line, and the trailing caret line —
    /// **without** a leading `error:` (or `warning:` / `note:`) header
    /// and **without** a trailing newline. The rendered body opens
    /// with a leading `\n` so consumers concatenating it after their
    /// own header (`"error: {msg}"`, `"warning: {msg}"`, a
    /// JSON-emitter's structured `level` field, an LSP surface's
    /// `Diagnostic` prefix, a `note:` companion pinned under a
    /// different column of the SAME source line) see the location
    /// on the next line — matching rustc's snippet posture.
    ///
    /// `label` is the file path or any identifier the caller wants in
    /// the `--> label:line:col` line; pass `None` when there is no
    /// source name (the REPL, an in-memory string) and the location
    /// renders as `--> line N, column M`.
    ///
    /// Pre-lift the snippet body was assembled inline inside
    /// [`format_diagnostic`] via three [`infallible_writeln!`] sites
    /// plus one [`infallible_write!`] site. Consumers wanting the
    /// same body under their own header (a `warning:` variant, a
    /// `note:` companion, a JSON emitter, an LSP surface) had no
    /// first-class named renderer to reach for — they either
    /// re-implemented the four emission sites (silently drifting the
    /// gutter width, the `-->` arrow, the pipe separator, or the
    /// tab-mirror discipline the `caret_pad` field already inherits)
    /// or substring-parsed [`format_diagnostic`]'s rendered snippet
    /// after stripping its `error: <msg>` prefix. Post-lift this
    /// method is the ONE named snippet-body renderer every
    /// non-`error` diagnostic surface composes over; a regression at
    /// the snippet template (an added gutter char, a swapped arrow
    /// glyph, a trailing-newline drift) reaches every consumer
    /// through ONE edit at the substrate boundary.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform /
    /// constructive diagnostics. The rustc-style snippet template is
    /// named as a first-class substrate primitive so downstream
    /// header-owning consumers (warnings, notes, JSON emitters, LSP)
    /// cannot silently drift the gutter/arrow/pipe/caret shape from
    /// the substrate. THEORY.md §VI.1 — generation over composition;
    /// the four-emission-site snippet template inside
    /// [`format_diagnostic`] is named as ONE typed renderer every
    /// future header-owning emitter binds to rather than re-derives.
    #[must_use]
    pub fn render_snippet_body(&self, label: Option<&str>) -> String {
        let line_str = self.line.to_string();
        let gutter = " ".repeat(line_str.len());
        let Self {
            line,
            column,
            line_text,
            caret_pad,
        } = self;

        let mut out = String::new();
        out.push('\n');
        match label {
            Some(label) => infallible_writeln!(out, "{gutter}--> {label}:{line}:{column}"),
            None => infallible_writeln!(out, "{gutter}--> line {line}, column {column}"),
        }
        infallible_writeln!(out, "{gutter} |");
        infallible_writeln!(out, "{line_str} | {line_text}");
        infallible_write!(out, "{gutter} | {caret_pad}^");
        out
    }
}

/// Convert a byte offset into a 1-based `LineCol`. Offsets past EOF
/// clamp to the final position. `column` counts UTF-8 scalar
/// characters, not bytes — an `é` is one column, two bytes — so the
/// caret renders under the visible character a human sees.
#[must_use]
pub fn line_col(src: &str, byte_offset: usize) -> LineCol {
    let cap = byte_offset.min(src.len());
    let mut line = 1usize;
    let mut column = 1usize;
    let mut idx = 0usize;
    for c in src.chars() {
        if idx >= cap {
            break;
        }
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
        idx += c.len_utf8();
    }
    LineCol { line, column }
}

/// Slice the line of `src` containing `byte_offset` (without its
/// trailing `\n`). Used by `format_diagnostic` to render the caret
/// underneath the right line.
fn line_at(src: &str, byte_offset: usize) -> &str {
    let cap = byte_offset.min(src.len());
    let start = src[..cap].rfind('\n').map_or(0, |i| i + 1);
    let end = src[start..].find('\n').map_or(src.len(), |i| start + i);
    &src[start..end]
}

/// Build a caret-pad string whose rendered visual width equals the
/// first `column - 1` chars of `line_text` under a fixed-width terminal.
///
/// Each source `\t` mirrors through as `\t`; every other char becomes
/// a space. This preserves caret alignment for tab-indented sources —
/// the pad and the source line consume the same tab-stops, so the
/// caret lands under the offending byte regardless of the terminal's
/// tab-stop setting. Pre-lift the caret pad was
/// `" ".repeat(column.saturating_sub(1))`, which silently drifted
/// under a tab-indented source (a `\t` renders as N columns of source
/// but as ONE space in the pad, so the caret slid left of the byte).
///
/// Named at the substrate level so a future range-underline
/// diagnostic (e.g. `let caret = mirror + "^".repeat(width)` for a
/// multi-column highlight, or a `note:` companion line pinned under a
/// different column of the SAME source line) inherits the tab-mirror
/// discipline mechanically — the visual-width invariant lives at ONE
/// projection on the diagnostic-rendering surface.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / constructive
/// diagnostics. A caret whose column drifts under a tab-indented
/// source is not knowable to the operator. Inspiration: rustc's
/// `SnippetData::render_source_line` tab-mirror idiom; translation
/// through pleme-io primitives is a chars-iterator over the source
/// line already in hand, no new IR layer.
fn mirror_source_prefix_as_pad(line_text: &str, column: usize) -> String {
    line_text
        .chars()
        .take(column.saturating_sub(1))
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect()
}

/// Render a `LispError` as a rustc-style diagnostic with a caret.
///
/// ```text
/// error: unmatched closing paren at position 3
///  --> file.lisp:1:4
///   |
/// 1 |    )
///   |    ^
/// ```
///
/// `label` is the file path or any identifier the caller wants in the
/// `--> label:line:col` line; pass `None` when there is no source name
/// (the REPL, an in-memory string) and the location renders as
/// `--> line N, column M`.
///
/// Errors whose `position()` is `None` (`Type`, `Compile`, …) render
/// as a single `error: <msg>` line — there is nothing to point at.
/// As more variants gain positions, those errors automatically pick
/// up the snippet rendering with no consumer changes.
#[must_use]
pub fn format_diagnostic(src: &str, err: &LispError, label: Option<&str>) -> String {
    let mut out = format!("error: {err}");
    let Some(pos) = err.position() else {
        return out;
    };
    out.push_str(&SourceProjection::at(src, pos).render_snippet_body(label));
    out
}

#[cfg(test)]
mod tests {
    use super::{
        format_diagnostic, line_at, line_col, mirror_source_prefix_as_pad, LineCol,
        SourceProjection,
    };
    use crate::error::LispError;
    use crate::reader::read;

    // ── line_col ────────────────────────────────────────────────────

    #[test]
    fn line_col_at_start_of_input() {
        assert_eq!(line_col("abc", 0), LineCol { line: 1, column: 1 });
    }

    #[test]
    fn line_col_advances_columns_on_first_line() {
        assert_eq!(line_col("abc", 1), LineCol { line: 1, column: 2 });
        assert_eq!(line_col("abc", 2), LineCol { line: 1, column: 3 });
    }

    #[test]
    fn line_col_at_eof_is_one_past_last_char() {
        assert_eq!(line_col("abc", 3), LineCol { line: 1, column: 4 });
    }

    #[test]
    fn line_col_clamps_past_eof() {
        assert_eq!(line_col("abc", 999), LineCol { line: 1, column: 4 });
        assert_eq!(line_col("", 999), LineCol { line: 1, column: 1 });
    }

    #[test]
    fn line_col_advances_line_after_newline() {
        // `a\nb` — offset 0 = (1,1); 1 = (1,2) (still on line 1, after `a`);
        // 2 = (2,1) (after the `\n`); 3 = (2,2) (after `b`).
        assert_eq!(line_col("a\nb", 0), LineCol { line: 1, column: 1 });
        assert_eq!(line_col("a\nb", 1), LineCol { line: 1, column: 2 });
        assert_eq!(line_col("a\nb", 2), LineCol { line: 2, column: 1 });
        assert_eq!(line_col("a\nb", 3), LineCol { line: 2, column: 2 });
    }

    #[test]
    fn line_col_counts_chars_not_bytes_for_multibyte() {
        // `é` is two bytes (0xC3 0xA9) but one column. Offset = 2 lands
        // immediately after `é`, i.e. column 2 on line 1.
        assert_eq!(line_col("é", 2), LineCol { line: 1, column: 2 });
        assert_eq!(line_col("\né", 1), LineCol { line: 2, column: 1 });
        assert_eq!(line_col("\né", 3), LineCol { line: 2, column: 2 });
    }

    // ── line_at ─────────────────────────────────────────────────────

    #[test]
    fn line_at_returns_the_containing_line_without_newline() {
        let src = "alpha\nbeta\ngamma";
        assert_eq!(line_at(src, 0), "alpha");
        assert_eq!(line_at(src, 6), "beta"); // first char of line 2
        assert_eq!(line_at(src, 11), "gamma"); // first char of line 3
        assert_eq!(line_at(src, 16), "gamma"); // EOF still on line 3
    }

    // ── format_diagnostic ───────────────────────────────────────────

    #[test]
    fn format_diagnostic_renders_unmatched_paren_with_caret_under_offending_byte() {
        // `   )` — stray `)` at byte 3, which is column 4 on line 1.
        // The caret under the `)` proves the column math + line slicing
        // agree.
        let src = "   )";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("x.lisp"));
        let expected = "\
error: unmatched closing paren at position 3
 --> x.lisp:1:4
  |
1 |    )
  |    ^";
        assert_eq!(rendered, expected, "got:\n{rendered}");
    }

    #[test]
    fn format_diagnostic_locates_paren_on_a_later_line() {
        // Two leading lines plus a stray `)` — confirms the line index
        // and the line-slicing both work past the first newline.
        let src = "(a b)\n(c d)\n   )\n";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("nested.lisp"));
        // The stray `)` is at byte 15 → (line 3, column 4).
        let expected = "\
error: unmatched closing paren at position 15
 --> nested.lisp:3:4
  |
3 |    )
  |    ^";
        assert_eq!(rendered, expected, "got:\n{rendered}");
    }

    #[test]
    fn format_diagnostic_unmatched_open_points_at_the_unclosed_paren() {
        // `(a (b c` — inner `(` at byte 3 is the deepest unclosed open.
        let src = "(a (b c";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("open.lisp"));
        let expected = "\
error: unmatched opening paren at position 3
 --> open.lisp:1:4
  |
1 | (a (b c
  |    ^";
        assert_eq!(rendered, expected, "got:\n{rendered}");
    }

    #[test]
    fn format_diagnostic_omits_label_when_none() {
        let err = read(")").unwrap_err();
        let rendered = format_diagnostic(")", &err, None);
        // No file path is known; still produce a structured location.
        let expected = "\
error: unmatched closing paren at position 0
 --> line 1, column 1
  |
1 | )
  | ^";
        assert_eq!(rendered, expected, "got:\n{rendered}");
    }

    #[test]
    fn format_diagnostic_renders_eof_at_end_of_input() {
        // `(a b) '` — trailing quote with no datum runs the parser past
        // EOF; the caret renders one column past the last visible char.
        let src = "(a b) '";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("dangle.lisp"));
        let expected = "\
error: unexpected end of input at position 7
 --> dangle.lisp:1:8
  |
1 | (a b) '
  |        ^";
        assert_eq!(rendered, expected, "got:\n{rendered}");
    }

    #[test]
    fn infallible_writeln_macro_appends_formatted_line_with_trailing_newline() {
        // Pin the macro's emission shape: `writeln!`-equivalent into a
        // `String`, no swallowed bytes, no missing newline. A regression
        // that drops the newline or mis-handles format-arg interpolation
        // fails-loudly here. The macro is the centralized substitute
        // for the four inline `.expect("writes to a String never
        // fail")` triples that recurred in `format_diagnostic`'s body
        // pre-lift.
        let mut out = String::new();
        infallible_writeln!(out, "hello {x}", x = 42);
        assert_eq!(out, "hello 42\n");
    }

    #[test]
    fn infallible_write_macro_appends_formatted_text_without_newline() {
        // Sibling of `infallible_writeln!` — non-newline-terminated
        // emission. Pin that the macro does NOT add a trailing newline
        // so the caret-line rendering in `format_diagnostic` stays
        // byte-for-byte stable. A regression that adds a newline here
        // fails-loudly via the existing `format_diagnostic_*` tests
        // AND this isolated unit-pin.
        let mut out = String::new();
        infallible_write!(out, "tail {y}", y = "value");
        assert_eq!(out, "tail value");
    }

    #[test]
    fn infallible_macros_preserve_format_diagnostic_byte_identity() {
        // The lift is a pure refactor — `format_diagnostic`'s rendered
        // output must be byte-for-byte identical to the pre-lift state
        // across every existing test case. The five `format_diagnostic_*`
        // tests below already pin specific expected strings; this test
        // re-asserts that path-uniformity at the macro-substitution
        // layer: emit one full diagnostic and confirm both the caret
        // line (the only `infallible_write!` site) AND the gutter
        // lines (three `infallible_writeln!` sites) render correctly
        // together.
        let src = "   )";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("macros.lisp"));
        assert!(rendered.starts_with("error: unmatched closing paren"));
        assert!(rendered.contains("\n --> macros.lisp:1:4\n"));
        assert!(rendered.ends_with("^"));
        assert!(
            !rendered.ends_with("^\n"),
            "trailing caret line must NOT emit a newline (would drift consumer concat)"
        );
    }

    #[test]
    fn format_diagnostic_falls_back_to_single_line_for_positionless_errors() {
        // A `Compile` error has no position today; it must still render
        // as a clean single line so downstream tools can dump it
        // unconditionally.
        let err = LispError::Compile {
            form: ":threshold".into(),
            message: "expected number".into(),
        };
        let rendered = format_diagnostic("(defmonitor :threshold #t)", &err, Some("m.lisp"));
        assert_eq!(
            rendered,
            "error: compile error in :threshold: expected number"
        );
        assert!(
            !rendered.contains('\n'),
            "single-line render must not introduce newlines"
        );
        assert!(
            !rendered.contains('^'),
            "no caret allowed without a position to point at"
        );
    }

    // ── mirror_source_prefix_as_pad ─────────────────────────────────
    //
    // The pre-lift `" ".repeat(column - 1)` caret pad drifted under a
    // tab-indented source (a `\t` renders as N columns of source but
    // as ONE space in the pad, so the caret slid left of the offending
    // byte). Post-lift the pad mirrors each source char — tabs stay
    // tabs, everything else becomes a space — so the pad and the
    // source line consume the SAME tab-stops on a fixed-width terminal.
    // The pins below anchor the four canonical fixpoints AND the
    // end-to-end composition through `format_diagnostic`.

    #[test]
    fn mirror_source_prefix_as_pad_at_column_one_is_empty() {
        // Column 1 means the caret sits under the FIRST char of the
        // source line — zero pad ahead of it. `saturating_sub(1)` guards
        // both column 0 (unreachable but defensively OK) and column 1.
        assert_eq!(mirror_source_prefix_as_pad("(a b)", 1), "");
        assert_eq!(mirror_source_prefix_as_pad("(a b)", 0), "");
    }

    #[test]
    fn mirror_source_prefix_as_pad_replaces_non_tab_chars_with_spaces() {
        // A tab-free source line reproduces the pre-lift behavior byte-
        // for-byte: N chars of source before the caret → N spaces of
        // pad. Load-bearing for the existing `format_diagnostic_*`
        // tests, which all pin space-only prefixes.
        assert_eq!(mirror_source_prefix_as_pad("   )", 4), "   ");
        assert_eq!(mirror_source_prefix_as_pad("(a b c)", 5), "    ");
        assert_eq!(
            mirror_source_prefix_as_pad("hello", 6),
            "     ",
            "column past-last-char pads with spaces for every source char",
        );
    }

    #[test]
    fn mirror_source_prefix_as_pad_preserves_tabs_verbatim() {
        // A single leading tab mirrors through as a tab — the caret pad
        // consumes the same tab-stop the source did, so `\t)` and `\t^`
        // land the `)` and the `^` at the SAME visual column regardless
        // of the terminal's tab-stop setting (2, 4, 8, whatever).
        assert_eq!(mirror_source_prefix_as_pad("\t)", 2), "\t");
        assert_eq!(mirror_source_prefix_as_pad("\t\t)", 3), "\t\t");
    }

    #[test]
    fn mirror_source_prefix_as_pad_mirrors_mixed_tab_and_space_prefix() {
        // Real-world indent shapes (space-then-tab, tab-then-space,
        // interleaved) must reproduce the source's exact whitespace
        // sequence in the pad. A regression that converts tabs to
        // spaces (or vice versa) fails HERE with a visible mismatch.
        assert_eq!(mirror_source_prefix_as_pad("  \t)", 4), "  \t");
        assert_eq!(mirror_source_prefix_as_pad("\t  )", 4), "\t  ");
        // `(` is a non-tab char — it becomes a space in the pad while
        // the surrounding tabs mirror through as tabs. Pin that the
        // interleaved `[tab, space, non-tab, tab]` prefix produces
        // `[tab, space, space, tab]` so the caret's tab-stop advances
        // remain aligned regardless of what non-tab chars precede it.
        assert_eq!(mirror_source_prefix_as_pad("\t (\t)", 5), "\t  \t");
    }

    #[test]
    fn mirror_source_prefix_as_pad_replaces_multibyte_chars_with_spaces() {
        // `é` is one char, one column. The pad counts CHARS (matching
        // `line_col`'s `column` accounting), so `é)` at column 2 →
        // ONE space of pad, not two (which the pre-lift byte-repeat
        // would have produced under a naive byte-count).
        assert_eq!(mirror_source_prefix_as_pad("é)", 2), " ");
        assert_eq!(mirror_source_prefix_as_pad("\téé)", 4), "\t  ");
    }

    #[test]
    fn mirror_source_prefix_as_pad_clamps_to_line_length_at_eof_column() {
        // `format_diagnostic_renders_eof_at_end_of_input` renders the
        // caret one column past the last visible char. Under this
        // logic the pad is `chars().take(N)` over an N-char line, which
        // yields exactly N mirrored chars — same as the pre-lift
        // `" ".repeat(N)` for a tab-free source. Pin the clamp so a
        // future refactor that swaps `take` for a slice-index panics
        // out at rustc / test time rather than silently mispadding
        // EOF errors.
        assert_eq!(mirror_source_prefix_as_pad("(a b) '", 8), "       ");
        assert_eq!(mirror_source_prefix_as_pad("\tfoo", 5), "\t   ");
    }

    #[test]
    fn format_diagnostic_caret_pad_mirrors_tab_indent_for_terminal_alignment() {
        // END-TO-END CONTRACT: a tab-indented source with a stray `)`
        // must render a caret pad whose leading tab matches the source
        // line's leading tab — so the terminal displays `^` under `)`
        // regardless of tab-stop setting. Pre-lift this rendered
        // `\t)` above `  ^` (two spaces where a tab belongs), which
        // slid the caret left of the `)` on every real terminal. Pin
        // the fix at the outer boundary so a regression in
        // `mirror_source_prefix_as_pad` surfaces through the diagnostic
        // consumer, not just its internal unit-pin.
        let src = "\t)";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("tabby.lisp"));
        let expected = "\
error: unmatched closing paren at position 1
 --> tabby.lisp:1:2
  |
1 | \t)
  | \t^";
        assert_eq!(rendered, expected, "got:\n{rendered}");
    }

    #[test]
    fn format_diagnostic_caret_pad_mirrors_mixed_tab_space_indent() {
        // Deeper composition: mixed leading indent (space + tab +
        // space) with the caret on a nested `(` that stays unclosed.
        // Every prefix char round-trips into the pad — spaces stay
        // spaces, the tab stays a tab. A regression that homogenizes
        // the prefix to all-spaces fails HERE with a visible mismatch
        // on the tab position.
        let src = " \t (a b";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("mixed.lisp"));
        // Unclosed `(` sits at byte 3, column 4 on line 1.
        let expected = "\
error: unmatched opening paren at position 3
 --> mixed.lisp:1:4
  |
1 |  \t (a b
  |  \t ^";
        assert_eq!(rendered, expected, "got:\n{rendered}");
    }

    // ── SourceProjection::at — the byte-offset → rendered-components
    // typed projection every diagnostic renderer needs. Sibling of
    // `line_col` (the LineCol-only projection); `SourceProjection::at`
    // is the composed 4-tuple projection that stacks the containing-
    // line slice + the tab-mirrored caret pad ON TOP of the LineCol
    // walk in ONE typed call. Pre-lift these components were computed
    // inline at `format_diagnostic`'s body; consumers wanting the same
    // projection outside `format_diagnostic` had no first-class named
    // projection and either re-implemented `line_at` +
    // `mirror_source_prefix_as_pad` (silently drifting the tab-mirror
    // discipline) or substring-parsed `format_diagnostic`'s snippet.
    // The pins below anchor each component and the composition law at
    // the substrate boundary — fail-before-pass-after: the struct +
    // constructor did not exist pre-lift, so these tests cannot even
    // compile against the pre-lift API surface.

    #[test]
    fn source_projection_at_binds_each_field_to_its_sibling_helper() {
        // COMPOSITION LAW: `SourceProjection::at(src, pos)` returns a
        // struct whose four fields agree BYTE-FOR-BYTE with the three
        // sibling helpers evaluated at the same (src, pos). Pin the
        // composition rule end-to-end so a regression that specialized
        // ONE component (added a per-fleet salt to `line`, sliced
        // `line_text` by byte offset rather than through `line_at`,
        // padded the caret with spaces rather than mirroring tabs)
        // would surface HERE rather than as silent caret drift at
        // every downstream consumer. Sweeps representative shapes:
        // tab-free, tab-leading, multi-line, multibyte character.
        for (src, pos) in [
            ("   )", 3),                  // stray `)`, column 4 line 1
            ("(a b)\n(c d)\n   )\n", 15), // stray `)`, line 3
            ("\t)", 1),                   // tab-indent, column 2 line 1
            ("é)", 2),                    // multibyte prefix
            (" \t (a b", 3),              // mixed tab+space
        ] {
            let proj = SourceProjection::at(src, pos);
            let LineCol { line, column } = line_col(src, pos);
            assert_eq!(proj.line, line, "line drift for src={src:?} pos={pos}");
            assert_eq!(
                proj.column, column,
                "column drift for src={src:?} pos={pos}"
            );
            assert_eq!(
                proj.line_text,
                line_at(src, pos),
                "line_text drift for src={src:?} pos={pos}",
            );
            assert_eq!(
                proj.caret_pad,
                mirror_source_prefix_as_pad(line_at(src, pos), column),
                "caret_pad drift for src={src:?} pos={pos}",
            );
        }
    }

    #[test]
    fn source_projection_at_preserves_tab_mirror_discipline_end_to_end() {
        // DISCIPLINE PIN: a `\t)` source projected at byte 1 (the `)`)
        // MUST yield a caret_pad whose leading char is `\t`, not a
        // space. Pre-lift consumers reaching for the same projection
        // OUTSIDE `format_diagnostic` had to reimplement the tab-
        // mirror rule; post-lift they compose through this projection
        // and inherit the discipline mechanically. A regression that
        // silently reverted to `" ".repeat(column - 1)` (the pre-lift
        // caret pad's shape) would slide the caret left of the `)` on
        // every real terminal — this pin surfaces the drift AT the
        // typed projection rather than only at `format_diagnostic`'s
        // rendered-snippet consumer.
        let proj = SourceProjection::at("\t)", 1);
        assert_eq!(proj.line, 1);
        assert_eq!(proj.column, 2);
        assert_eq!(proj.line_text, "\t)");
        assert_eq!(proj.caret_pad, "\t");
    }

    #[test]
    fn source_projection_at_clamps_offsets_past_eof_to_final_position() {
        // EOF-CLAMP PIN: `SourceProjection::at` inherits the clamp
        // corner from `line_col` + `line_at`. An offset past the end
        // of the source returns the projection at the final position,
        // NOT a panic. Every downstream consumer that renders an
        // `Eof`-shaped diagnostic (a dangling quote, an unterminated
        // string) relies on this corner.
        let src = "(a b) '";
        let proj = SourceProjection::at(src, 999);
        assert_eq!(proj.line, 1);
        assert_eq!(proj.column, 8); // one past the trailing `'`
        assert_eq!(proj.line_text, "(a b) '");
        assert_eq!(proj.caret_pad, "       "); // seven spaces, no tab
    }

    #[test]
    fn source_projection_at_projects_multibyte_column_as_char_count_not_byte_count() {
        // MULTIBYTE PIN: `é` is one char, two bytes. A projection at
        // byte 2 (immediately after `é`) MUST land on column 2 on
        // line 1 — and the caret_pad MUST hold exactly ONE space, not
        // two (which the pre-lift byte-count spelling would have
        // produced). Sibling of `line_col_counts_chars_not_bytes_for_
        // multibyte` at the composed-projection level.
        let proj = SourceProjection::at("é)", 2);
        assert_eq!(proj.line, 1);
        assert_eq!(proj.column, 2);
        assert_eq!(proj.line_text, "é)");
        assert_eq!(proj.caret_pad, " ");
    }

    #[test]
    fn source_projection_at_borrows_line_text_from_source_lifetime() {
        // LIFETIME PIN: `line_text` borrows from `src` verbatim (same
        // lifetime as `line_at`), so a consumer that holds the source
        // string can retain the projection without copying the line.
        // A regression that specialized `line_text` to an owned
        // `String` (a defensive `.to_string()`) would break this pin
        // via the type system — this test only compiles when the
        // borrow lifetime holds. Pin the invariant explicitly so
        // future refactors that widen the field's shape fail-loud at
        // rustc rather than silently forcing a per-projection
        // allocation on every diagnostic renderer.
        let src = String::from("alpha\nbeta");
        let proj = SourceProjection::at(&src, 0);
        let borrowed: &str = proj.line_text;
        assert_eq!(borrowed, "alpha");
        assert!(
            std::ptr::eq(borrowed.as_ptr(), src.as_ptr()),
            "line_text must borrow directly from src, not allocate"
        );
    }

    // ── SourceProjection::render_snippet_body — the rustc-style snippet
    // template lifted OUT of `format_diagnostic` onto the typed
    // projection. Sibling of [`Self::at`] (the byte-offset →
    // rendered-components projection); `render_snippet_body` composes
    // ON TOP OF the projection to emit the location line + gutter +
    // source line + caret line as ONE named renderer every
    // header-owning consumer (a `warning:` variant, a `note:`
    // companion, a JSON emitter, an LSP surface) binds to rather than
    // re-implementing the four-emission-site template. Pre-lift these
    // consumers either re-implemented the template (silently drifting
    // the gutter width, the arrow glyph, the pipe separator, or the
    // tab-mirror discipline) or substring-parsed `format_diagnostic`'s
    // snippet after stripping "error: <msg>". The pins below anchor
    // the composition law, the leading-newline posture, the
    // no-trailing-newline posture, the labelless fallback, the
    // tab-mirror discipline inheritance, and the gutter-width rule at
    // the substrate boundary — fail-before-pass-after: the method did
    // not exist pre-lift, so these tests cannot even compile against
    // the pre-lift API surface.

    #[test]
    fn render_snippet_body_matches_format_diagnostic_snippet_after_stripping_header() {
        // COMPOSITION LAW: `render_snippet_body` returns exactly the
        // substring `format_diagnostic` appends after its
        // `"error: {msg}"` header. A regression that split the
        // composition (e.g. `format_diagnostic` re-inlined the four
        // emission sites rather than routing through the projection's
        // renderer) would silently drift the snippet body between the
        // two paths — this pin surfaces the drift at the substrate
        // boundary.
        for (src, label) in [
            ("   )", Some("x.lisp")),
            ("(a b)\n(c d)\n   )\n", Some("nested.lisp")),
            ("\t)", Some("tabby.lisp")),
            (")", None),
            ("(a b) '", Some("dangle.lisp")),
        ] {
            let err = read(src).unwrap_err();
            let rendered = format_diagnostic(src, &err, label);
            let header = format!("error: {err}");
            let body_via_format = rendered
                .strip_prefix(&header)
                .expect("format_diagnostic starts with its 'error: {msg}' header");
            let pos = err.position().expect("reader error carries a position");
            let proj = SourceProjection::at(src, pos);
            let body_via_render = proj.render_snippet_body(label);
            assert_eq!(
                body_via_format, body_via_render,
                "snippet body drifted between format_diagnostic and \
                 SourceProjection::render_snippet_body for src={src:?} label={label:?}"
            );
        }
    }

    #[test]
    fn render_snippet_body_opens_with_leading_newline() {
        // LEADING-NEWLINE PIN: the snippet body must begin with `\n`
        // so a consumer concatenating it after `"error: {msg}"` (or
        // `"warning: {msg}"` / a JSON-emitter's `level` field) sees
        // the location on the next line — matching rustc's snippet
        // posture. A regression that dropped the leading `\n` would
        // squash the location onto the header line and reads as a
        // one-liner formatting bug at every downstream consumer.
        let proj = SourceProjection::at("   )", 3);
        let body = proj.render_snippet_body(Some("x.lisp"));
        assert!(
            body.starts_with('\n'),
            "snippet body must open with `\\n`; got: {body:?}"
        );
    }

    #[test]
    fn render_snippet_body_ends_at_caret_without_trailing_newline() {
        // NO-TRAILING-NEWLINE PIN: the snippet body's last char is
        // `^`, not a newline — so a consumer chaining a `note:`
        // companion line (or a second snippet) onto the same string
        // controls the exact separator (a blank line, an inline gap,
        // no separator at all). Sibling of
        // `infallible_write_macro_appends_formatted_text_without_newline`
        // at the composed-snippet level.
        let proj = SourceProjection::at("   )", 3);
        let body = proj.render_snippet_body(Some("x.lisp"));
        assert!(
            body.ends_with('^'),
            "snippet body must end at the caret; got: {body:?}"
        );
        assert!(
            !body.ends_with("^\n"),
            "snippet body must NOT emit a trailing newline (would drift consumer concat); \
             got: {body:?}"
        );
    }

    #[test]
    fn render_snippet_body_omits_label_when_none() {
        // LABELLESS PIN: with `label = None` the location line reads
        // `--> line N, column M` (matching `format_diagnostic`'s
        // labelless fallback). Pin the fallback at the substrate
        // boundary so a REPL / in-memory-string consumer picks up
        // the same shape as `format_diagnostic` without needing to
        // route through the "error: <msg>" header.
        let proj = SourceProjection::at(")", 0);
        let body = proj.render_snippet_body(None);
        let expected = "\
\n --> line 1, column 1
  |
1 | )
  | ^";
        assert_eq!(body, expected, "got:\n{body}");
    }

    #[test]
    fn render_snippet_body_emits_labelled_location_line_when_some() {
        // LABELLED PIN: with `label = Some(l)` the location line
        // reads `--> l:line:col` (matching `format_diagnostic`'s
        // labelled shape). Pin the labelled emission at the substrate
        // boundary so a consumer routing its own path label into the
        // snippet picks up the same shape as `format_diagnostic`.
        let proj = SourceProjection::at("   )", 3);
        let body = proj.render_snippet_body(Some("x.lisp"));
        let expected = "\
\n --> x.lisp:1:4
  |
1 |    )
  |    ^";
        assert_eq!(body, expected, "got:\n{body}");
    }

    #[test]
    fn render_snippet_body_inherits_tab_mirror_discipline() {
        // DISCIPLINE INHERITANCE PIN: the snippet body's caret line
        // reproduces the `caret_pad` field verbatim — a tab-indented
        // source produces a caret line whose leading char is `\t`,
        // not a space. A regression that specialized the caret line
        // (e.g. replaced `{caret_pad}` with a space-repeat) would
        // slide the caret left of the offending byte on tab-indented
        // sources at every renderer routing through this method.
        // Sibling of `format_diagnostic_caret_pad_mirrors_tab_indent_
        // for_terminal_alignment` at the header-agnostic snippet
        // level.
        let proj = SourceProjection::at("\t)", 1);
        let body = proj.render_snippet_body(Some("tabby.lisp"));
        let expected = "\
\n --> tabby.lisp:1:2
  |
1 | \t)
  | \t^";
        assert_eq!(body, expected, "got:\n{body}");
    }

    #[test]
    fn render_snippet_body_gutter_width_tracks_line_number_digit_count() {
        // GUTTER-WIDTH PIN: the gutter (the leading space run before
        // `-->`, ` |`, and ` |` on the caret line) is
        // `" ".repeat(line.to_string().len())` — one space per line-
        // number digit. A regression that hard-coded the gutter width
        // to one space would mis-align the `-->` arrow on multi-digit
        // line numbers, and the snippet's rendered snippet would look
        // ragged. Pin the invariant on a two-digit line-number source
        // to surface a hard-coded-width regression that a single-line
        // fixture would miss.
        let mut src = String::new();
        for _ in 0..11 {
            src.push_str("(a)\n");
        }
        src.push_str("   )"); // stray `)` on line 12, byte offset 47.
        let pos = src.len() - 1;
        let proj = SourceProjection::at(&src, pos);
        assert_eq!(proj.line, 12, "sanity: line-12 fixture");
        let body = proj.render_snippet_body(Some("wide.lisp"));
        let expected = "\
\n  --> wide.lisp:12:4
   |
12 |    )
   |    ^";
        assert_eq!(body, expected, "got:\n{body}");
    }

    #[test]
    fn render_snippet_body_composes_under_warning_header() {
        // HEADER-AGNOSTIC PIN: the snippet body is header-agnostic —
        // a `warning:` consumer prepends its own header and the
        // rendered output has the same shape as `format_diagnostic`
        // modulo the header prefix. This is the whole point of the
        // lift: a warning / note / JSON / LSP consumer composes ON TOP
        // OF this method rather than re-implementing the four-
        // emission-site template. Pin the composition on the smallest
        // meaningful header change so a regression at the snippet
        // template surfaces on every header-owning consumer.
        let src = "   )";
        let err = read(src).unwrap_err();
        let pos = err.position().expect("reader error carries a position");
        let proj = SourceProjection::at(src, pos);
        let warning = format!("warning: {err}{}", proj.render_snippet_body(Some("x.lisp")));
        let expected = "\
warning: unmatched closing paren at position 3
 --> x.lisp:1:4
  |
1 |    )
  |    ^";
        assert_eq!(warning, expected, "got:\n{warning}");
    }

    #[test]
    fn format_diagnostic_composes_over_source_projection_at() {
        // END-TO-END COMPOSITION PIN: `format_diagnostic` routes
        // through `SourceProjection::at` for its four rendered
        // components. Pin the composition by rendering the same
        // diagnostic BOTH through `format_diagnostic` AND by hand
        // through `SourceProjection::at`'s fields — the two must
        // agree on the snippet body. A regression that split the
        // composition (e.g. `format_diagnostic` re-inlined the three
        // sibling helpers rather than routing through the projection)
        // would silently drift the caret placement or the containing-
        // line slice between the two paths.
        let src = "\t)";
        let err = read(src).unwrap_err();
        let rendered = format_diagnostic(src, &err, Some("tabby.lisp"));
        let pos = err.position().expect("reader error carries a position");
        let proj = SourceProjection::at(src, pos);
        let expected_body = format!(
            "\n --> tabby.lisp:{line}:{col}\n  |\n{line} | {text}\n  | {pad}^",
            line = proj.line,
            col = proj.column,
            text = proj.line_text,
            pad = proj.caret_pad,
        );
        assert!(
            rendered.ends_with(&expected_body),
            "format_diagnostic snippet drifted from SourceProjection::at:\n\
             got:\n{rendered}\n\nexpected suffix:\n{expected_body}",
        );
    }
}
