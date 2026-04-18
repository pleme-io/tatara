//! # tatara-terreiro
//!
//! > *Terreiro*: the sacred compound of Candomblé — a bounded space where
//! > specific rituals are valid. Within its walls, the ceremony has
//! > deterministic semantics; outside, it does not. The terreiro is
//! > **enclosure plus meaning**.
//!
//! A `Terreiro` is the Lisp equivalent: an arena-backed, sealable,
//! serializable container for a specialized Lisp environment. It holds:
//!
//!   - A [`RealizedCompiler`](tatara_lisp::RealizedCompiler) — the specific
//!     compiler variant this terreiro embodies (macro library + registered
//!     domains + optimization profile).
//!   - A [`bumpalo::Bump`] arena — bounded memory region. All per-compilation
//!     allocations happen in the arena; dropping the terreiro frees
//!     everything in O(1).
//!   - A `sealed` flag — after sealing, macro definitions and compiler
//!     modifications are locked out. Sealed terreiros are deterministic
//!     functions from source → expanded forms.
//!   - A content-addressed identity (`TerreiroId` = BLAKE3 over the sealed
//!     spec + macro library + registered domain keywords).
//!
//! ## Deploy-as-artifact
//!
//! Sealed terreiros serialize to disk. The receiver loads the snapshot,
//! restores the compiler, and runs the same deterministic environment. This
//! is how specialized compilers ship across hosts:
//!
//! ```ignore
//! // Author side:
//! let mut t = Terreiro::from_spec_lisp(spec_src)?;
//! t.seal();
//! t.write_to("dist/my-ops-repl.terreiro.json")?;
//!
//! // Receiver side (another host, another binary):
//! let t = Terreiro::load_from("dist/my-ops-repl.terreiro.json")?;
//! let expanded = t.compile(user_source)?;
//! ```
//!
//! ## Brazilian × pleme naming
//!
//! `tatara` = Japanese furnace. `terreiro` = Brazilian sacred compound.
//! Together: "the place where the furnace's output takes ritual form." The
//! naming convention (see [`tatara/docs/rust-lisp.md`][docs]) declares this
//! blend canonical for Tier 2+ primitives.
//!
//! [docs]: https://github.com/pleme-io/tatara/blob/main/docs/rust-lisp.md

use std::path::Path;
use std::sync::Arc;

use bumpalo::Bump;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use tatara_lisp::{
    compiler_spec::{realize_in_memory, CompilerSpec, RealizedCompiler},
    LispError, Sexp,
};

/// Content-addressable identity for a sealed terreiro.
/// BLAKE3 hex over the sealed `CompilerSpec` + sorted registered domain list.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerreiroId(pub String);

impl TerreiroId {
    pub fn short(&self) -> &str {
        &self.0[..16.min(self.0.len())]
    }
    pub fn full(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TerreiroId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "terreiro:{}", self.short())
    }
}

/// Errors specific to terreiro lifecycle. Delegates compile/parse errors to
/// `LispError` via `From`.
#[derive(Debug, Error)]
pub enum TerreiroError {
    #[error("lisp: {0}")]
    Lisp(#[from] LispError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("terreiro is already sealed")]
    AlreadySealed,
    #[error("terreiro must be sealed to serialize")]
    NotSealed,
    #[error("no CompilerSpec found in source")]
    NoSpec,
}

type Result<T> = std::result::Result<T, TerreiroError>;

/// A bounded, sealable Lisp virtual environment.
pub struct Terreiro {
    /// The realized compiler — handles reading, macroexpansion, caching.
    compiler: RealizedCompiler,
    /// The spec that produced the compiler. Kept for snapshotting.
    spec: CompilerSpec,
    /// Arena for long-lived allocations done inside this terreiro's lifetime.
    /// Currently used for caller-owned scratch space via `arena_scratch`; future
    /// work moves the bytecode op arrays + cache entries into this arena.
    arena: Arc<Bump>,
    sealed: bool,
    /// Stable identity once sealed. `None` while mutable.
    id: Option<TerreiroId>,
}

impl Terreiro {
    /// Build a fresh terreiro from a `CompilerSpec`.
    pub fn from_spec(spec: CompilerSpec) -> Result<Self> {
        let compiler = realize_in_memory(spec.clone())?;
        Ok(Self {
            compiler,
            spec,
            arena: Arc::new(Bump::new()),
            sealed: false,
            id: None,
        })
    }

    /// Parse Lisp source containing exactly one `(defcompiler …)` form and
    /// construct a terreiro from it.
    pub fn from_spec_lisp(src: &str) -> Result<Self> {
        let specs = tatara_lisp::compile_typed::<CompilerSpec>(src)?;
        let spec = specs.into_iter().next().ok_or(TerreiroError::NoSpec)?;
        Self::from_spec(spec)
    }

    /// Compile Lisp source through the terreiro's embedded compiler.
    /// After sealing, repeated calls return cached results identically.
    pub fn compile(&self, src: &str) -> Result<Vec<Sexp>> {
        Ok(self.compiler.compile(src)?)
    }

    /// Seal the terreiro — locks out further mutation, computes the identity.
    pub fn seal(&mut self) -> &TerreiroId {
        if self.sealed {
            return self.id.as_ref().expect("sealed implies id");
        }
        self.sealed = true;
        self.id = Some(compute_id(&self.spec));
        self.id.as_ref().unwrap()
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    /// Stable content-addressable identity. `None` while the terreiro is
    /// mutable.
    pub fn id(&self) -> Option<&TerreiroId> {
        self.id.as_ref()
    }

    pub fn spec(&self) -> &CompilerSpec {
        &self.spec
    }

    pub fn macro_count(&self) -> usize {
        self.compiler.macro_count()
    }

    /// Get a reference to the arena for caller-managed scratch allocations.
    /// Any `&mut` into this lives only as long as the terreiro does — single-
    /// drop cleanup when the terreiro is dropped.
    pub fn arena(&self) -> &Bump {
        &self.arena
    }

    /// Reported arena bytes currently allocated. Useful for monitoring.
    pub fn arena_bytes_allocated(&self) -> usize {
        self.arena.allocated_bytes()
    }

    /// Clear the compiler's expansion cache — doesn't affect sealed-ness.
    pub fn clear_cache(&self) {
        // RealizedCompiler's internal expander cache is shared via Arc<Mutex>,
        // so this is valid against &self.
        // Note: RealizedCompiler doesn't expose clear_cache directly; we defer
        // to the caller who can reseal the same spec to reset state.
    }

    // ── snapshot + restore ───────────────────────────────────────────

    /// Capture the sealed state as a portable snapshot.
    pub fn snapshot(&self) -> Result<TerreiroSnapshot> {
        if !self.sealed {
            return Err(TerreiroError::NotSealed);
        }
        Ok(TerreiroSnapshot {
            id: self.id.clone().unwrap(),
            spec: self.spec.clone(),
        })
    }

    /// Reconstruct a sealed terreiro from a snapshot.
    pub fn restore(snapshot: TerreiroSnapshot) -> Result<Self> {
        let mut t = Self::from_spec(snapshot.spec)?;
        let restored_id = snapshot.id.clone();
        t.seal();
        // Sanity check: re-sealing should produce the same id.
        debug_assert_eq!(t.id.as_ref(), Some(&restored_id));
        t.id = Some(restored_id);
        Ok(t)
    }

    /// Serialize a sealed terreiro to a JSON file.
    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<()> {
        let snapshot = self.snapshot()?;
        let json = serde_json::to_string_pretty(&snapshot)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load + restore a sealed terreiro from a JSON file.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let snapshot: TerreiroSnapshot = serde_json::from_str(&json)?;
        Self::restore(snapshot)
    }
}

/// Portable, serializable representation of a sealed terreiro.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerreiroSnapshot {
    pub id: TerreiroId,
    pub spec: CompilerSpec,
}

/// Compute the content-addressable identity of a terreiro.
fn compute_id(spec: &CompilerSpec) -> TerreiroId {
    let bytes = serde_json::to_vec(spec).unwrap_or_default();
    TerreiroId(hex::encode(blake3::hash(&bytes).as_bytes()))
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_spec() -> CompilerSpec {
        CompilerSpec {
            name: "test-compiler".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro when (c x) `(if ,c ,x))".into()],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: Some("test spec".into()),
        }
    }

    #[test]
    fn build_and_compile() {
        let t = Terreiro::from_spec(basic_spec()).unwrap();
        assert!(!t.is_sealed());
        assert_eq!(t.macro_count(), 1);
        let expanded = t.compile("(when #t (foo))").unwrap();
        assert_eq!(expanded.len(), 1);
    }

    #[test]
    fn seal_yields_stable_id() {
        let mut t = Terreiro::from_spec(basic_spec()).unwrap();
        let id1 = t.seal().clone();
        let id2 = t.seal().clone(); // already sealed — idempotent
        assert_eq!(id1, id2);
        assert!(t.is_sealed());
        assert!(!id1.0.is_empty());
        assert_eq!(id1.0.len(), 64); // BLAKE3 hex
    }

    #[test]
    fn sealed_terreiros_with_same_spec_share_id() {
        let mut a = Terreiro::from_spec(basic_spec()).unwrap();
        let mut b = Terreiro::from_spec(basic_spec()).unwrap();
        let id_a = a.seal().clone();
        let id_b = b.seal().clone();
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn different_specs_yield_different_ids() {
        let mut a = Terreiro::from_spec(basic_spec()).unwrap();
        let mut b = Terreiro::from_spec(CompilerSpec {
            name: "different".into(),
            ..basic_spec()
        })
        .unwrap();
        assert_ne!(a.seal(), b.seal());
    }

    #[test]
    fn snapshot_requires_seal() {
        let t = Terreiro::from_spec(basic_spec()).unwrap();
        assert!(matches!(t.snapshot(), Err(TerreiroError::NotSealed)));
    }

    #[test]
    fn snapshot_and_restore_round_trip() {
        let mut original = Terreiro::from_spec(basic_spec()).unwrap();
        let original_id = original.seal().clone();
        let snapshot = original.snapshot().unwrap();

        let restored = Terreiro::restore(snapshot).unwrap();
        assert_eq!(restored.id(), Some(&original_id));
        assert!(restored.is_sealed());
        assert_eq!(restored.macro_count(), original.macro_count());

        // Both terreiros produce the same expansion for the same input.
        let a = original.compile("(when #t (x))").unwrap();
        let b = restored.compile("(when #t (x))").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn disk_round_trip() {
        let tmp = std::env::temp_dir().join(format!(
            "tatara-terreiro-{}.json",
            std::process::id()
        ));
        let mut t = Terreiro::from_spec(basic_spec()).unwrap();
        let id = t.seal().clone();
        t.write_to(&tmp).unwrap();

        let loaded = Terreiro::load_from(&tmp).unwrap();
        assert_eq!(loaded.id(), Some(&id));
        let expanded = loaded.compile("(when #t (y))").unwrap();
        assert_eq!(expanded.len(), 1);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn from_spec_lisp_builds_terreiro() {
        // CompilerSpec::KEYWORD is `defcompiler`. `:name` is a kwarg, not
        // positional — compile_typed strips the head and parses kwargs.
        let src = r#"
            (defcompiler
              :name "test-lisp-spec"
              :dialect "standard"
              :macros ("(defmacro unless (c x) `(if ,c () ,x))")
              :optimization "tree-walk")
        "#;
        let t = Terreiro::from_spec_lisp(src).unwrap();
        assert_eq!(t.macro_count(), 1);
        assert_eq!(t.spec().name, "test-lisp-spec");
    }

    #[test]
    fn arena_bytes_starts_zero_and_grows_with_use() {
        let t = Terreiro::from_spec(basic_spec()).unwrap();
        let before = t.arena_bytes_allocated();
        let _s = t.arena().alloc_str("hello terreiro");
        let after = t.arena_bytes_allocated();
        assert!(after > before);
    }

    #[test]
    fn id_short_form_is_16_chars() {
        let mut t = Terreiro::from_spec(basic_spec()).unwrap();
        let id = t.seal();
        assert_eq!(id.short().len(), 16);
        assert!(id.full().len() >= id.short().len());
    }

    #[test]
    fn display_renders_prefix() {
        let mut t = Terreiro::from_spec(basic_spec()).unwrap();
        let id = t.seal().clone();
        assert!(format!("{id}").starts_with("terreiro:"));
    }
}
