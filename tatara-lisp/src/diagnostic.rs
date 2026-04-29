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

use std::fmt::Write as _;

use crate::error::LispError;

/// 1-based line + column. `line_col` walks the source up to a byte
/// offset; `\n` increments `line` and resets `column` to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
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
    let LineCol { line, column } = line_col(src, pos);
    let line_text = line_at(src, pos);
    let line_str = line.to_string();
    let gutter = " ".repeat(line_str.len());
    let caret_pad = " ".repeat(column.saturating_sub(1));

    out.push('\n');
    match label {
        Some(label) => writeln!(out, "{gutter}--> {label}:{line}:{column}"),
        None => writeln!(out, "{gutter}--> line {line}, column {column}"),
    }
    .expect("writes to a String never fail");
    writeln!(out, "{gutter} |").expect("writes to a String never fail");
    writeln!(out, "{line_str} | {line_text}").expect("writes to a String never fail");
    write!(out, "{gutter} | {caret_pad}^").expect("writes to a String never fail");
    out
}

#[cfg(test)]
mod tests {
    use super::{format_diagnostic, line_at, line_col, LineCol};
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
}
