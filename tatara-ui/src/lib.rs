//! # tatara-ui — Nord-themed CLI UX for the tatara toolchain
//!
//! Three layers, every one content-addressable:
//!
//! 1. **Palette** ([`palette`]) — canonical Nord values + semantic `Role` vocabulary
//!    mapped via `RoleMap::default()`.
//! 2. **Sigils** ([`sigil`]) — typed Unicode glyphs (❄ ✓ ✗ ⟡ → ● ◇ ⚡ ⚙ …) each
//!    bound to a default `Role`.
//! 3. **Events** ([`event`]) — `UiEvent` is a typed enum that serializes to
//!    tatara-lisp Sexps. A whole run is an `EventStream`; BLAKE3 of the stream
//!    JSON is the **run-identity hash** that `tatara replay <hash>` uses.
//!
//! Plus the cache-aware fun:
//!
//! - **Themes are derivations**: `(deftheme nord-arctic …)` parses via
//!   `#[derive(TataraDomain)]`; `ThemeSpec::id()` is a BLAKE3 of the spec.
//! - **Every artifact line shows `⚡ cached` or `⚙ built 5.3s`** next to a
//!   `◇ blake3:cxx3i50` short hash, so cache hits tell a visual story.
//! - **Summary banner** at the end shows the content-root hash + `{built,
//!   cached, failed, total}` counts.
//!
//! ```lisp
//! (deftheme nord-arctic
//!   :description "the canonical tatara look"
//!   :semantic    (:primary "#88C0D0"
//!                 :success "#A3BE8C"
//!                 :warn    "#EBCB8B"
//!                 :error   "#BF616A"))
//! ```

extern crate self as tatara_ui;

pub mod event;
pub mod palette;
pub mod render;
pub mod sigil;
pub mod theme;

pub use event::{ArtifactState, Cell, EventStream, LogLevel, ShortHash, UiEvent};
pub use palette::{Role, RoleMap, Rgb, NORD};
pub use render::{should_color, Renderer};
pub use sigil::Sigil;
pub use theme::{SemanticOverrides, ThemeId, ThemeRegistry, ThemeSpec};
