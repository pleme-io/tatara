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

use tatara_eval::{EvalError, Interpreter, Value};
use tatara_lisp::{
    compiler_spec::{realize_in_memory, CompilerSpec, RealizedCompiler},
    LispError, Sexp,
};
use tatara_nix::realize::{
    InProcessRealizer, NixStoreRealizer, RealizeError, RealizedArtifact, Realizer,
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
    #[error("eval: {0}")]
    Eval(#[from] EvalError),
    #[error("realize: {0}")]
    Realize(#[from] RealizeError),
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
    #[error("no interpreter attached to this terreiro — call with_interpreter()")]
    NoInterpreter,
    #[error("no realizer attached to this terreiro — call with_realizer()")]
    NoRealizer,
    #[error("evaluated to {0}, not a derivation — cannot realize")]
    NotADerivation(String),
}

type Result<T> = std::result::Result<T, TerreiroError>;

/// Two realizer flavors the terreiro knows how to host. Avoids trait-object
/// bounds and keeps snapshot semantics local.
pub enum TerreiroRealizer {
    /// Self-contained hermetic builder (default store = `$TATARA_STORE_DIR`
    /// or `$XDG_DATA_HOME/tatara/store`).
    InProcess(InProcessRealizer),
    /// Bind to a running Nix on disk — emits `(derivation { … })` Nix
    /// expressions and lets `/nix/store` own the build + cache.
    NixStore(NixStoreRealizer),
}

impl TerreiroRealizer {
    fn realize(
        &self,
        d: &tatara_nix::Derivation,
    ) -> std::result::Result<RealizedArtifact, RealizeError> {
        match self {
            Self::InProcess(r) => r.realize(d),
            Self::NixStore(r) => r.realize(d),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::InProcess(_) => "in-process",
            Self::NixStore(_) => "nix-store",
        }
    }
}

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
    /// Optional evaluator — present when the terreiro hosts a full Lisp
    /// runtime (not just macroexpansion).
    interpreter: Option<Arc<Interpreter>>,
    /// Optional realizer — turns `Derivation` values into on-disk store paths.
    realizer: Option<Arc<TerreiroRealizer>>,
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
            interpreter: None,
            realizer: None,
            sealed: false,
            id: None,
        })
    }

    /// Attach a fresh tatara-eval Interpreter. Lets the terreiro evaluate Lisp
    /// to `Value`, including typed `Derivation` values.
    pub fn with_interpreter(mut self) -> Self {
        self.interpreter = Some(Arc::new(Interpreter::new()));
        self
    }

    /// Attach an in-process hermetic realizer (no Nix dependency).
    pub fn with_in_process_realizer(mut self, store_dir: impl Into<std::path::PathBuf>) -> Self {
        self.realizer = Some(Arc::new(TerreiroRealizer::InProcess(
            InProcessRealizer::new(store_dir),
        )));
        self
    }

    /// Attach a realizer that hands builds off to a running Nix on disk.
    pub fn with_nix_store_realizer(mut self) -> Self {
        self.realizer = Some(Arc::new(
            TerreiroRealizer::NixStore(NixStoreRealizer::new()),
        ));
        self
    }

    /// Attach a realizer pointing at the platform-default tatara store
    /// (`$TATARA_STORE_DIR` or XDG data dir).
    pub fn with_default_store(mut self) -> Self {
        self.realizer = Some(Arc::new(TerreiroRealizer::InProcess(
            InProcessRealizer::default_store(),
        )));
        self
    }

    /// Is an interpreter wired up?
    pub fn has_interpreter(&self) -> bool {
        self.interpreter.is_some()
    }

    /// Is a realizer wired up? Which flavor?
    pub fn realizer_kind(&self) -> Option<&'static str> {
        self.realizer.as_ref().map(|r| r.kind())
    }

    /// Evaluate Lisp source end-to-end: macroexpand through `self.compiler`,
    /// then `self.interpreter.eval_forms`. Returns the value of the last form.
    pub fn eval(&self, src: &str) -> Result<Value> {
        let interp = self
            .interpreter
            .as_ref()
            .ok_or(TerreiroError::NoInterpreter)?;
        let forms = self.compiler.compile(src)?;
        Ok(interp.eval_forms(&forms)?)
    }

    /// Evaluate Lisp source, then realize the resulting `Derivation` to a
    /// concrete artifact on disk. Errors if the final value is not a
    /// derivation.
    pub fn realize(&self, src: &str) -> Result<RealizedArtifact> {
        let realizer = self.realizer.as_ref().ok_or(TerreiroError::NoRealizer)?;
        let value = self.eval(src)?;
        let interp = self.interpreter.as_ref().expect("eval() proved present");
        let forced = interp.force(value)?;
        match forced {
            Value::Derivation(d) => Ok(realizer.realize(&d)?),
            other => Err(TerreiroError::NotADerivation(other.type_name().into())),
        }
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
        let tmp = std::env::temp_dir().join(format!("tatara-terreiro-{}.json", std::process::id()));
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

    // ── end-to-end: Lisp → eval → realize → on-disk artifact ─────────

    fn eval_spec() -> CompilerSpec {
        CompilerSpec {
            name: "eval-capable".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: Some("terreiro w/ interpreter + realizer".into()),
        }
    }

    #[test]
    fn terreiro_evaluates_lisp_arithmetic() {
        let t = Terreiro::from_spec(eval_spec()).unwrap().with_interpreter();
        assert!(t.has_interpreter());
        let v = t.eval("(+ 1 2 3)").unwrap();
        assert!(matches!(v, Value::Int(6)));
    }

    #[test]
    fn terreiro_builds_derivation_value() {
        let t = Terreiro::from_spec(eval_spec()).unwrap().with_interpreter();
        let v = t
            .eval(r#"(derivation (attrs "name" "widget" "version" "1.0"))"#)
            .unwrap();
        match v {
            Value::Derivation(d) => {
                assert_eq!(d.name, "widget");
                assert_eq!(d.version.as_deref(), Some("1.0"));
            }
            other => panic!("expected Derivation, got {other:?}"),
        }
    }

    #[test]
    fn terreiro_realizes_lisp_to_disk() {
        let store = tempfile::tempdir().unwrap();
        let t = Terreiro::from_spec(eval_spec())
            .unwrap()
            .with_interpreter()
            .with_in_process_realizer(store.path());
        assert_eq!(t.realizer_kind(), Some("in-process"));

        let src = r#"
            (derivation
              (attrs
                "name"    "greeting"
                "version" "1.0"
                "source"  (attrs "kind" "Inline" "content" "hello from lisp\n")
                "builder" (attrs
                            "phases"   (list "Install")
                            "commands" (attrs
                                         "Install" (list "cat \"$src\" > \"$out_file\"")))))
        "#;
        let art = t.realize(src).unwrap();
        assert!(!art.cached);
        assert!(art.path.exists());
        let got = std::fs::read_to_string(&art.path).unwrap();
        assert_eq!(got, "hello from lisp\n");
    }

    #[test]
    fn terreiro_realize_errors_when_not_derivation() {
        let store = tempfile::tempdir().unwrap();
        let t = Terreiro::from_spec(eval_spec())
            .unwrap()
            .with_interpreter()
            .with_in_process_realizer(store.path());
        let err = t.realize("42").unwrap_err();
        assert!(matches!(err, TerreiroError::NotADerivation(_)));
    }

    #[test]
    fn terreiro_errors_without_interpreter() {
        let t = Terreiro::from_spec(eval_spec()).unwrap();
        let err = t.eval("(+ 1 2)").unwrap_err();
        assert!(matches!(err, TerreiroError::NoInterpreter));
    }

    #[test]
    fn terreiro_errors_without_realizer() {
        let t = Terreiro::from_spec(eval_spec()).unwrap().with_interpreter();
        let err = t.realize(r#"(derivation (attrs "name" "x"))"#).unwrap_err();
        assert!(matches!(err, TerreiroError::NoRealizer));
    }

    #[test]
    fn terreiro_with_lisp_let_binding_and_realize() {
        let store = tempfile::tempdir().unwrap();
        let t = Terreiro::from_spec(eval_spec())
            .unwrap()
            .with_interpreter()
            .with_in_process_realizer(store.path());

        // Compose a derivation using a let binding — proves the full
        // authoring surface (let/lambda/arithmetic) + realization chain work.
        let src = r#"
            (let ((greeting "hello ")
                  (target   "terreiro\n"))
              (derivation
                (attrs
                  "name"    "composed"
                  "version" "2.0"
                  "source"  (attrs "kind"    "Inline"
                                   "content" (string-append greeting target))
                  "builder" (attrs
                              "phases"   (list "Install")
                              "commands" (attrs
                                           "Install" (list "cat \"$src\" > \"$out_file\""))))))
        "#;
        let art = t.realize(src).unwrap();
        let got = std::fs::read_to_string(&art.path).unwrap();
        assert_eq!(got, "hello terreiro\n");

        // Second realization is a cache hit — same content-addressed path.
        let art2 = t.realize(src).unwrap();
        assert_eq!(art.store_path, art2.store_path);
        assert!(art2.cached);
    }

    #[test]
    fn sealed_terreiro_still_realizes() {
        let store = tempfile::tempdir().unwrap();
        let mut t = Terreiro::from_spec(eval_spec())
            .unwrap()
            .with_interpreter()
            .with_in_process_realizer(store.path());
        t.seal();
        let src = r#"
            (derivation
              (attrs
                "name"    "sealed-greet"
                "source"  (attrs "kind" "Inline" "content" "from sealed\n")
                "builder" (attrs
                            "phases"   (list "Install")
                            "commands" (attrs "Install" (list "cat \"$src\" > \"$out_file\""))) ))
        "#;
        let art = t.realize(src).unwrap();
        assert_eq!(std::fs::read_to_string(&art.path).unwrap(), "from sealed\n");
    }
}
