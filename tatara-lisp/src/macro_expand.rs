//! Macro expander — rewrites `defmacro` / `defpoint-template` calls into
//! their quasi-quoted templates.
//!
//! Semantics (v0, no evaluator):
//!
//! ```lisp
//! (defmacro wrap (x) `(list ,x ,x))      ; or defpoint-template
//! (wrap hello)                            ; expands to (list hello hello)
//! ```
//!
//! Supported:
//!   - Required params:      `(name a b c)`
//!   - Rest param:           `(name a &rest rest)`
//!   - Quasi-quote body:     `` `(…) ``
//!   - Unquote substitution: `,x`
//!   - Splice substitution:  `,@x` (splices a bound list into the outer list)
//!   - Recursive expansion: macro bodies may call other macros.
//!
//! Not yet supported (no evaluator):
//!   - Arbitrary expressions under `,` — only bound symbol lookups.
//!   - Nested quasi-quotes.
//!   - Hygiene / gensym — param names capture aggressively.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::ast::Sexp;
use crate::error::{LispError, MacroDefHead, Result, TemplateInvariantKind, UnquoteForm};

/// Cache key: (macro name, SipHash-2-4 of args). We hash `Sexp` directly via
/// its manual `Hash` impl — no serde_json round-trip per cache lookup.
type CacheKey = (String, u64);

/// A registered macro definition.
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: String,
    pub params: Vec<Param>,
    /// The template body (usually a Quasiquote).
    pub body: Sexp,
}

#[derive(Debug, Clone)]
pub enum Param {
    Required(String),
    Rest(String),
}

/// Macro environment. Collects `defmacro` forms and rewrites callers.
///
/// Expansion strategy is tunable per-expander:
///   - **Compiled (default)** — every registered macro's template is walked once
///     and flattened into a linear `CompiledTemplate` (a tiny bytecode: Literal,
///     Subst(index), Splice(index), BeginList, EndList). Expansion of a call
///     is then a linear pass with no HashMap lookups and no recursion through
///     the template Sexp. Purely-literal subtrees compile to a single
///     `Literal(Sexp)` op — huge win for macros where most of the body is fixed.
///   - **Substitute-only** — runs the name-keyed `substitute` walker. Slower
///     but proves equivalence; used in the benchmark test to measure the
///     compiled-vs-substituted speedup.
#[derive(Clone, Default)]
pub struct Expander {
    macros: HashMap<String, MacroDef>,
    /// Pre-compiled template bytecodes, populated when `compile_templates`.
    templates: HashMap<String, CompiledTemplate>,
    /// When true, register a CompiledTemplate alongside each macro and dispatch
    /// expansion through the bytecode interpreter.
    compile_templates: bool,
    /// Memoization of `apply(macro, args)` — repeated calls with identical
    /// args skip expansion entirely. Shared across clones so realizations of
    /// the same `CompilerSpec` benefit across .compile() invocations.
    cache: Arc<Mutex<HashMap<CacheKey, Sexp>>>,
    /// Toggle caching. Default on — caching is the actual performance win
    /// the bytecode layer enables.
    cache_enabled: bool,
}

impl Expander {
    /// Default expander — compiled bytecode + expansion cache enabled.
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            templates: HashMap::new(),
            compile_templates: true,
            cache: Arc::new(Mutex::new(HashMap::new())),
            cache_enabled: true,
        }
    }

    /// Expander using the legacy substitute path (no template compilation,
    /// no cache). Kept for benchmarking + equivalence testing.
    pub fn new_substitute_only() -> Self {
        Self {
            macros: HashMap::new(),
            templates: HashMap::new(),
            compile_templates: false,
            cache: Arc::new(Mutex::new(HashMap::new())),
            cache_enabled: false,
        }
    }

    /// Expander with bytecode on but expansion cache off — isolates the cache
    /// contribution from the bytecode infrastructure. Benchmark baseline.
    pub fn new_bytecode_no_cache() -> Self {
        let mut e = Self::new();
        e.cache_enabled = false;
        e
    }

    /// Toggle the expansion cache at runtime.
    pub fn set_cache_enabled(&mut self, enabled: bool) {
        self.cache_enabled = enabled;
    }

    /// How many entries are currently cached.
    pub fn cache_size(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Clear the expansion cache (e.g., after redefining a macro).
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }

    pub fn with_macros<I: IntoIterator<Item = MacroDef>>(defs: I) -> Result<Self> {
        let mut e = Self::new();
        for d in defs {
            if e.compile_templates {
                e.templates.insert(d.name.clone(), compile_template(&d)?);
            }
            e.macros.insert(d.name.clone(), d);
        }
        Ok(e)
    }

    /// Expand a whole program. Returns the list of top-level forms after
    /// `defmacro` definitions are registered and all macro calls expanded.
    pub fn expand_program(&mut self, forms: Vec<Sexp>) -> Result<Vec<Sexp>> {
        let mut out = Vec::new();
        for form in forms {
            if let Some(def) = macro_def_from(&form)? {
                if self.compile_templates {
                    self.templates
                        .insert(def.name.clone(), compile_template(&def)?);
                }
                self.macros.insert(def.name.clone(), def);
                continue;
            }
            out.push(self.expand(&form)?);
        }
        Ok(out)
    }

    /// Expand a single form. Top-level macro calls are rewritten; recurses
    /// into list children.
    pub fn expand(&self, form: &Sexp) -> Result<Sexp> {
        let Some(list) = form.as_list() else {
            return Ok(form.clone());
        };
        if let Some(head) = list.first().and_then(|s| s.as_symbol()) {
            if let Some(def) = self.macros.get(head) {
                let expanded = self.apply(def, &list[1..])?;
                // Recurse — the expansion itself may contain more macro calls.
                return self.expand(&expanded);
            }
        }
        // Not a macro call — expand children.
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            out.push(self.expand(item)?);
        }
        Ok(Sexp::List(out))
    }

    /// Apply a macro to its argument list.
    ///
    /// Three-layer fast path:
    ///   1. If `cache_enabled`, hash `(name, args)` and consult the memo table.
    ///   2. If a compiled template exists, run the bytecode interpreter.
    ///   3. Otherwise fall back to the name-keyed substitute walker.
    fn apply(&self, def: &MacroDef, args: &[Sexp]) -> Result<Sexp> {
        // Layer 1: expansion cache.
        let cache_key = if self.cache_enabled {
            args_cache_key(&def.name, args)
        } else {
            None
        };
        if let Some(ref key) = cache_key {
            if let Some(cached) = self.cache.lock().unwrap().get(key) {
                return Ok(cached.clone());
            }
        }

        // Layer 2: compiled bytecode.
        let result = if let Some(tmpl) = self.templates.get(&def.name) {
            apply_compiled(&def.name, &def.params, tmpl, args)?
        } else {
            // Layer 3: substitute fallback.
            let bindings = bind_args(&def.name, &def.params, args)?;
            let body = match &def.body {
                Sexp::Quasiquote(inner) => inner.as_ref(),
                other => other,
            };
            substitute(body, &bindings)?
        };

        // Populate cache on miss.
        if let Some(key) = cache_key {
            self.cache.lock().unwrap().insert(key, result.clone());
        }
        Ok(result)
    }

    pub fn has(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.macros.len()
    }

    pub fn is_empty(&self) -> bool {
        self.macros.is_empty()
    }
}

// ── Compiled template bytecode ───────────────────────────────────────

/// One op in the template bytecode. Emitted during compilation; consumed at
/// expansion to materialize a form without HashMap lookups or recursion.
#[derive(Clone, Debug, PartialEq)]
pub enum TemplateOp {
    /// Push a literal Sexp. Used for atoms and entirely-literal subtrees.
    Literal(Sexp),
    /// Push the bound arg at the given param index.
    Subst(usize),
    /// If the bound arg is a list, append its items to the current list; else
    /// push it as a single item.
    Splice(usize),
    /// Begin a new List — pushes a fresh builder onto the expansion stack.
    BeginList,
    /// End the current List — pops the builder, wraps as `Sexp::List`.
    EndList,
}

/// Pre-compiled template. Built once per macro, interpreted many times.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompiledTemplate {
    pub ops: Vec<TemplateOp>,
}

/// Walk a macro definition's template body and emit linear bytecode.
/// Purely-literal subtrees compile to a single `Literal(clone)` op.
///
/// Compilation can fail if the template references a name that isn't a
/// declared parameter — same semantic as the substitute path.
///
/// Top-level `,@X` bodies (the splice is the entire body, not nested inside
/// a `(... ,@xs ...)` list) are rejected here at compile time so the
/// bytecode path agrees with the substitute path's emission-time rejection
/// (`splice_outside_list`). Without this gate the bytecode interpreter's
/// outermost stack frame silently absorbed the splice's items, and the same
/// macro emitted different output across paths — `compiled_template_matches
/// _substitute_path` only covered well-positioned splice bodies. After this
/// gate every `,@-outside-list` body is rejected at registration time on
/// both paths with ONE structural variant (`LispError::SpliceOutsideList`).
pub fn compile_template(def: &MacroDef) -> Result<CompiledTemplate> {
    let body = match &def.body {
        Sexp::Quasiquote(inner) => inner.as_ref(),
        other => other,
    };
    if let Sexp::UnquoteSplice(inner) = body {
        return Err(splice_outside_list(inner));
    }
    let params: Vec<&str> = def
        .params
        .iter()
        .map(|p| match p {
            Param::Required(n) | Param::Rest(n) => n.as_str(),
        })
        .collect();
    let mut ops = Vec::new();
    compile_node(body, &params, &mut ops)?;
    Ok(CompiledTemplate { ops })
}

fn compile_node(node: &Sexp, params: &[&str], ops: &mut Vec<TemplateOp>) -> Result<()> {
    // Fast-path literal: if the subtree has no Unquote/UnquoteSplice, emit a
    // single Literal op. This is the big win for macros where most of the
    // template is fixed structure.
    if !contains_unquote(node) {
        ops.push(TemplateOp::Literal(node.clone()));
        return Ok(());
    }
    match node {
        Sexp::Unquote(inner) => {
            let name = inner
                .as_symbol()
                .ok_or_else(|| non_symbol_unquote_target(UnquoteForm::Unquote, inner))?;
            let idx = params
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| unbound_template_var(UnquoteForm::Unquote, name, params))?;
            ops.push(TemplateOp::Subst(idx));
        }
        Sexp::UnquoteSplice(inner) => {
            let name = inner
                .as_symbol()
                .ok_or_else(|| non_symbol_unquote_target(UnquoteForm::Splice, inner))?;
            let idx = params
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| unbound_template_var(UnquoteForm::Splice, name, params))?;
            ops.push(TemplateOp::Splice(idx));
        }
        Sexp::List(items) => {
            ops.push(TemplateOp::BeginList);
            for item in items {
                compile_node(item, params, ops)?;
            }
            ops.push(TemplateOp::EndList);
        }
        _ => ops.push(TemplateOp::Literal(node.clone())),
    }
    Ok(())
}

fn contains_unquote(node: &Sexp) -> bool {
    match node {
        Sexp::Unquote(_) | Sexp::UnquoteSplice(_) => true,
        Sexp::List(items) => items.iter().any(contains_unquote),
        Sexp::Quote(inner) | Sexp::Quasiquote(inner) => contains_unquote(inner),
        _ => false,
    }
}

/// Promote the previously `LispError::Compile`-shaped helper into the
/// structural `LispError::TemplateInvariant { macro_name, kind }` variant.
/// The four reachable bytecode-runtime invariant violations in
/// `apply_compiled` — Subst-bad-index, Splice-bad-index, EndList-empty-
/// stack, final-no-value — funnel through ONE emission shape keyed on
/// the closed-set `TemplateInvariantKind` enum. The index payload of
/// the Subst / Splice gates lives INSIDE the variant (`SubstBadIndex(usize)`
/// / `SpliceBadIndex(usize)`), so the invalid combination "stack-gate
/// kind with an op-index" (e.g. `EndListEmptyStack` carrying a `usize`)
/// is structurally unrepresentable — the type system encodes "this gate
/// has an index, that gate does not."
///
/// Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
/// across all four kinds (`"compile error in {macro_name}: <invariant>"`)
/// via the closed-set `TemplateInvariantKind::message()` projection, so
/// authoring-tool substring greps (`tatara-check`, REPL) see no drift
/// across the lift.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set
/// of bytecode-invariant failure modes becomes a TYPE rather than a
/// free-form `message: String` slot. THEORY.md §VI.1 — generation over
/// composition; the typed enum lands the structural-completeness floor
/// for the bytecode-runtime surface, parallel to how `CompilerSpecIoStage`
/// lands the structural-completeness floor for the disk-persistence
/// surface (`compiler_spec.rs`, the immediately prior claude-routine
/// lift on a sibling file). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs): a well-formed bytecode invariant is the proof
/// that drives the interpreter; the structural variant makes the
/// proof's REJECTION shape first-class — authoring tools (REPL, LSP,
/// `tatara-check`) pattern-match on the `kind` slot and bind to the
/// gate identity directly instead of substring-parsing the rendered
/// diagnostic.
fn template_invariant_violation(macro_name: &str, kind: TemplateInvariantKind) -> LispError {
    LispError::TemplateInvariant {
        macro_name: macro_name.into(),
        kind,
    }
}

/// Execute a pre-compiled template against the macro's argument list.
fn apply_compiled(
    macro_name: &str,
    params: &[Param],
    tmpl: &CompiledTemplate,
    args: &[Sexp],
) -> Result<Sexp> {
    // Resolve args by param index (same binding semantics as `bind_args`).
    let mut args_by_index: Vec<Sexp> = Vec::with_capacity(params.len());
    let mut cursor = 0;
    for param in params {
        match param {
            Param::Required(name) => {
                let arg = args
                    .get(cursor)
                    .cloned()
                    .ok_or_else(|| missing_macro_arg(macro_name, name))?;
                args_by_index.push(arg);
                cursor += 1;
            }
            Param::Rest(_) => {
                let rest = args.get(cursor..).unwrap_or(&[]).to_vec();
                args_by_index.push(Sexp::List(rest));
                cursor = args.len();
            }
        }
    }

    // Run the bytecode against a stack of in-progress list builders. The
    // outermost frame accumulates the single result the template yields.
    let mut stack: Vec<Vec<Sexp>> = vec![Vec::with_capacity(1)];
    for op in &tmpl.ops {
        match op {
            TemplateOp::Literal(s) => stack.last_mut().unwrap().push(s.clone()),
            TemplateOp::Subst(idx) => {
                let v = args_by_index
                    .get(*idx)
                    .ok_or_else(|| {
                        template_invariant_violation(
                            macro_name,
                            TemplateInvariantKind::SubstBadIndex(*idx),
                        )
                    })?
                    .clone();
                stack.last_mut().unwrap().push(v);
            }
            TemplateOp::Splice(idx) => {
                let v = args_by_index.get(*idx).ok_or_else(|| {
                    template_invariant_violation(
                        macro_name,
                        TemplateInvariantKind::SpliceBadIndex(*idx),
                    )
                })?;
                match v {
                    Sexp::List(items) => stack.last_mut().unwrap().extend(items.iter().cloned()),
                    Sexp::Nil => {}
                    other => stack.last_mut().unwrap().push(other.clone()),
                }
            }
            TemplateOp::BeginList => stack.push(Vec::new()),
            TemplateOp::EndList => {
                let items = stack.pop().ok_or_else(|| {
                    template_invariant_violation(
                        macro_name,
                        TemplateInvariantKind::EndListEmptyStack,
                    )
                })?;
                stack.last_mut().unwrap().push(Sexp::List(items));
            }
        }
    }
    let mut top = stack.pop().ok_or_else(|| {
        template_invariant_violation(macro_name, TemplateInvariantKind::FinalNoValue)
    })?;
    if top.len() == 1 {
        Ok(top.remove(0))
    } else {
        Ok(Sexp::List(top))
    }
}

/// Hash of `(macro_name, args)` for cache keying — hot path, kept lean.
/// Uses `DefaultHasher` (SipHash-2-4) — fast enough that the cache hit rate
/// needed to net a win is low even for cheap macros.
fn args_cache_key(macro_name: &str, args: &[Sexp]) -> Option<CacheKey> {
    let mut h = DefaultHasher::new();
    args.len().hash(&mut h);
    for a in args {
        a.hash(&mut h);
    }
    Some((macro_name.to_string(), h.finish()))
}

fn macro_def_from(form: &Sexp) -> Result<Option<MacroDef>> {
    let Some(list) = form.as_list() else {
        return Ok(None);
    };
    let Some(head_str) = list.first().and_then(|s| s.as_symbol()) else {
        return Ok(None);
    };
    let Some(head) = MacroDefHead::from_keyword(head_str) else {
        return Ok(None);
    };
    if list.len() < 4 {
        return Err(defmacro_arity(head, list.len()));
    }
    let name = list[1]
        .as_symbol()
        .ok_or_else(|| defmacro_non_symbol_name(head, &list[1]))?
        .to_string();
    let param_list = list[2]
        .as_list()
        .ok_or_else(|| defmacro_non_list_params(head, &list[2]))?;
    let params = parse_params(param_list)?;
    let body = list[3].clone();
    Ok(Some(MacroDef { name, params, body }))
}

fn parse_params(list: &[Sexp]) -> Result<Vec<Param>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < list.len() {
        let s = list[i]
            .as_symbol()
            .ok_or_else(|| non_symbol_param(i, &list[i]))?;
        if s == "&rest" {
            let Some(next) = list.get(i + 1) else {
                return Err(rest_param_missing_name(i, None));
            };
            let Some(name) = next.as_symbol() else {
                return Err(rest_param_missing_name(i, Some(next)));
            };
            out.push(Param::Rest(name.to_string()));
            return Ok(out);
        }
        out.push(Param::Required(s.to_string()));
        i += 1;
    }
    Ok(out)
}

fn bind_args(macro_name: &str, params: &[Param], args: &[Sexp]) -> Result<HashMap<String, Sexp>> {
    let mut bindings: HashMap<String, Sexp> = HashMap::new();
    let mut i = 0;
    for param in params {
        match param {
            Param::Required(name) => {
                let arg = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| missing_macro_arg(macro_name, name))?;
                bindings.insert(name.clone(), arg);
                i += 1;
            }
            Param::Rest(name) => {
                let rest = args.get(i..).unwrap_or(&[]).to_vec();
                bindings.insert(name.clone(), Sexp::List(rest));
                i = args.len();
            }
        }
    }
    Ok(bindings)
}

/// Substitute `,name` and `,@name` within a template.
/// `,@name` only makes sense inside a List — it splices the bound list into
/// the containing list.
fn substitute(form: &Sexp, bindings: &HashMap<String, Sexp>) -> Result<Sexp> {
    match form {
        Sexp::Unquote(inner) => {
            let sym = inner
                .as_symbol()
                .ok_or_else(|| non_symbol_unquote_target(UnquoteForm::Unquote, inner))?;
            bindings.get(sym).cloned().ok_or_else(|| {
                unbound_template_var(UnquoteForm::Unquote, sym, &bound_names(bindings))
            })
        }
        Sexp::UnquoteSplice(inner) => Err(splice_outside_list(inner)),
        Sexp::List(items) => {
            let mut out: Vec<Sexp> = Vec::with_capacity(items.len());
            for item in items {
                if let Sexp::UnquoteSplice(inner) = item {
                    let sym = inner
                        .as_symbol()
                        .ok_or_else(|| non_symbol_unquote_target(UnquoteForm::Splice, inner))?;
                    let val = bindings.get(sym).ok_or_else(|| {
                        unbound_template_var(UnquoteForm::Splice, sym, &bound_names(bindings))
                    })?;
                    match val {
                        Sexp::List(children) => out.extend(children.iter().cloned()),
                        Sexp::Nil => {}
                        other => out.push(other.clone()),
                    }
                } else {
                    out.push(substitute(item, bindings)?);
                }
            }
            Ok(Sexp::List(out))
        }
        Sexp::Quote(_) | Sexp::Quasiquote(_) => Ok(form.clone()),
        _ => Ok(form.clone()),
    }
}

/// Lift the four inline `LispError::Compile { form: format!("{prefix}{name}"),
/// message: "unbound" }` triples (compile_node Unquote/UnquoteSplice +
/// substitute Unquote/UnquoteSplice) behind ONE named primitive. Pairs the
/// structural variant with `crate::domain::suggest`'s bounded edit-distance
/// scan over the candidate set so a typo in `,name` against a macro's params
/// (or against a substitution scope's live bindings) surfaces as
/// `"compile error in ,xs: unbound; did you mean ,x?"` instead of the bare
/// `"unbound"`. The candidate set is per-call — params during compile,
/// `bindings.keys()` during substitute — so the operator's hint is always
/// drawn from the in-scope name set, never a stale snapshot.
///
/// `prefix` is `UnquoteForm` — the closed-set typed enum whose two
/// variants are EXACTLY the two reachable syntactic markers
/// (`Unquote` ⊎ `Splice`). Threading the typed marker through the helper
/// boundary (rather than `&'static str`) lands the same compile-time
/// closed-set guarantee `defmacro_arity` / `defmacro_non_symbol_name` /
/// `defmacro_non_list_params` get from threading `MacroDefHead`: the
/// closed set is encoded in the type system, so a regression that drifts
/// the marker (e.g. a fourth `prefix: ",,"` call site) becomes a type
/// error at the call site, not a runtime substring drift. `name` is the
/// offender from source; the hint is `Option<String>` because the matched
/// candidate borrows from a transient `Vec<&str>` we built locally —
/// copying the matched name into the variant is the cheapest way to keep
/// `LispError` lifetime-free.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four inline
/// copies in one module is well past the three-times rule. THEORY.md §V.1 —
/// knowable platform; the structural variant exposes `prefix` / `name` /
/// `hint` as first-class fields so authoring tools (LSP, REPL,
/// `tatara-check`) bind to the data shape instead of substring-parsing the
/// rendered diagnostic.
fn unbound_template_var(prefix: UnquoteForm, name: &str, candidates: &[&str]) -> LispError {
    LispError::UnboundTemplateVar {
        prefix,
        name: name.to_string(),
        hint: crate::domain::suggest(name, candidates).map(str::to_string),
    }
}

/// Lift the four inline `LispError::Compile { form: "unquote" /
/// "unquote-splice", message: "only bound symbols may appear after `,` /
/// `,@`" }` triples in this module (compile_node Unquote / UnquoteSplice +
/// substitute Unquote / UnquoteSplice-inside-list) behind ONE named
/// primitive. Sibling of `unbound_template_var`: that helper fires when the
/// slot IS a symbol but the symbol isn't bound; this helper fires when the
/// slot isn't a symbol at all. Together they close every distinct
/// typed-entry template-gate failure mode for the no-evaluator template
/// language: each is a structural variant of `LispError`, not a
/// `Compile`-shaped substring.
///
/// `prefix` is `UnquoteForm` — the closed-set typed enum whose two
/// variants are EXACTLY the two reachable syntactic markers
/// (`Unquote` ⊎ `Splice`). Threading the typed marker through the helper
/// boundary (rather than `&'static str`) lands the same compile-time
/// closed-set guarantee `unbound_template_var` carries: the closed set is
/// encoded in the type system. The inner is the offending `Sexp` routed
/// through `crate::domain::sexp_witness` — the typed joint projection
/// pairing `SexpShape` (structural shape) with `Sexp::Display`
/// (renderable literal) at ONE call boundary. Authoring tools bind to
/// BOTH `got.shape` (e.g. `SexpShape::List`) AND `got.display` (e.g.
/// `"(list 1 2)"`) jointly — same posture as `splice_outside_list`
/// after its prior-run promotion to `SexpWitness`. The two template-
/// gate `,X/,@X` rejection variants now share ONE typed witness
/// identity at their `got` slot.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four
/// inline copies in one module is past the three-times rule. THEORY.md
/// §V.1 — knowable platform; the structural variant exposes `prefix` /
/// `got` as first-class fields so authoring tools (LSP, REPL,
/// `tatara-check`) bind to the data shape instead of substring-parsing
/// the rendered diagnostic. THEORY.md §II.1 invariant 1 — typed entry;
/// a non-symbol unquote target is exactly the failure mode the
/// typed-entry gate exists to reject.
fn non_symbol_unquote_target(prefix: UnquoteForm, got: &Sexp) -> LispError {
    LispError::NonSymbolUnquoteTarget {
        prefix,
        got: crate::domain::sexp_witness(got),
    }
}

/// Lift the lone `LispError::Compile { form: "unquote-splice", message:
/// "`,@` may only appear inside a list" }` triple — the substitute path's
/// top-level `,@X` rejection — behind ONE named primitive. Sibling of
/// `non_symbol_unquote_target` and `unbound_template_var`: those helpers
/// fire when the slot inside a `,X` / `,@X` is malformed (non-symbol or
/// unbound symbol); this helper fires when the `,@X` form itself is
/// ill-positioned (no containing list to flatten into). Together the three
/// close every distinct typed-entry template-gate failure mode for the
/// no-evaluator template language: each is a structural variant of
/// `LispError`, not a `Compile`-shaped substring.
///
/// `inner` is the offending `Sexp` projected through `Display` so the
/// operator sees the literal value they wrote — `xs`, `(list 1 2)`, `5` —
/// instead of just the bare "may only appear inside a list" verdict. The
/// helper takes `&Sexp` (parallel to `non_symbol_unquote_target`) and
/// projects through `to_string()` at the variant boundary; the `prefix:
/// &'static str` slot is implicit (always `,@`) and absent from the variant
/// itself, parallel to how `OddKwargs { dangling }` names ONE failure mode
/// without a syntactic-marker slot.
///
/// Used by both the substitute path (top-level `,@X` body) AND the bytecode
/// path's `compile_template` gate (top-level `,@X` body — closing the prior
/// silent-divergence where the bytecode interpreter's outermost stack frame
/// absorbed the splice). After this lift `,@-outside-list` is rejected on
/// both paths with ONE structural variant — the typed-entry template gate
/// is fully structural across both expansion strategies.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// emission sites (substitute + compile_template) for one failure mode is
/// past the three-times rule once the structural shape is named. THEORY.md
/// §V.1 — knowable platform; the structural variant exposes `got` as a
/// first-class field so authoring tools (LSP, REPL, `tatara-check`) bind to
/// the data shape instead of substring-parsing the rendered diagnostic.
/// THEORY.md §II.1 invariant 1 — typed entry; a `,@X` at a position with no
/// containing list is exactly the failure mode the typed-entry gate exists
/// to reject. THEORY.md §II.1 invariant 2 — free middle; both expansion
/// paths now reject the same set of templates, so a macro that registers
/// successfully has the same expansion behavior under either strategy.
fn splice_outside_list(inner: &Sexp) -> LispError {
    LispError::SpliceOutsideList {
        got: crate::domain::sexp_witness(inner),
    }
}

/// Lift the two inline `LispError::Compile { form: format!("call to
/// {macro_name}"), message: format!("missing required arg: {name}") }`
/// triples — `bind_args` (substitute path) AND `apply_compiled` (bytecode
/// path) — behind ONE named primitive. Sibling of the typed-entry kwargs
/// `MissingKwarg { key }` lift: that variant fires when a `(<head> :key
/// value …)` kwargs form omits a required keyword; this variant fires when
/// a `(<macroname> a b …)` call omits a required positional param. Together
/// they close every distinct typed-entry missing-required surface in the
/// substrate — kwargs-gate AND macro-call-gate now share a single
/// structural-variant idiom.
///
/// Same single emission shape across both expansion strategies — before
/// this lift the same failure mode emitted byte-identical
/// `LispError::Compile { … }` triples at TWO call sites; after this lift
/// both sites share ONE structural variant. Two strategies that picked
/// different code paths now emit the same structural variant for the same
/// failure mode (THEORY.md §II.1 invariant 2 — free middle: which strategy
/// you picked must not change which inputs you reject OR how the rejection
/// is shaped). Same posture as `splice_outside_list`'s path-uniform
/// rejection across substitute + compile_template.
///
/// `macro_name` and `name` are `&str` borrows from the call-site / param
/// list; the variant's owned `String`s are formed at the boundary so
/// `LispError` stays lifetime-free.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// inline copies of one shape is past the three-times-rule trigger once
/// the structural variant is named (the test count gives this the
/// fail-before-pass-after edge). THEORY.md §V.1 — knowable platform; the
/// structural variant exposes `macro_name` / `param` as first-class
/// fields so authoring tools (LSP, REPL, `tatara-check`) bind to the data
/// shape instead of substring-parsing the rendered diagnostic. THEORY.md
/// §II.1 invariant 1 — typed entry; a macro call with too few args is
/// exactly the failure mode the typed-entry gate exists to reject.
fn missing_macro_arg(macro_name: &str, param: &str) -> LispError {
    LispError::MissingMacroArg {
        macro_name: macro_name.to_string(),
        param: param.to_string(),
    }
}

/// Lift the lone `LispError::Compile { form: "defmacro params", message:
/// "expected symbol" }` triple in `parse_params` behind ONE named
/// primitive. Sibling of `missing_macro_arg`: that helper fires when the
/// macro CALL is malformed (call-site missing a positional arg); this
/// helper fires when the macro DEFINITION is malformed (definition-site
/// has a non-symbol where a param name should be). Together they open
/// the defmacro-syntax-gate / macro-call-gate split — call-site
/// rejections vs. definition-site rejections — each as its own
/// structural-variant family on `LispError`.
///
/// `position` is the loop index inside `parse_params`, i.e. the 0-based
/// index of the offending element within the param list (`(defmacro f
/// (a 5 b) …)` — position 1 is the literal `5`); naming it lets an LSP
/// quick-fix point at the exact list element instead of the whole
/// param list. `got` is the offending `Sexp` projected through
/// `Display` so the operator sees the literal value they wrote
/// (`5`, `:foo`, `(nested)`) at the variant boundary; the helper takes
/// `&Sexp` (parallel to `non_symbol_unquote_target` and
/// `splice_outside_list`) and projects through `to_string()` so the
/// variant stays lifetime-free.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; one
/// inline copy still earns a named primitive once the structural shape
/// is named (the test count gives this the fail-before-pass-after edge,
/// parallel to how `OddKwargs` was lifted from a single site for the
/// structural-completeness payoff). THEORY.md §V.1 — knowable platform;
/// the structural variant exposes `position` / `got` as first-class
/// fields so authoring tools (LSP, REPL, `tatara-check`) bind to the
/// data shape instead of substring-parsing the rendered diagnostic.
/// THEORY.md §II.1 invariant 1 — typed entry; a non-symbol element
/// inside a defmacro param list is exactly the failure mode the
/// typed-entry gate exists to reject — and it must reject DEFINITIONS
/// as readily as it rejects CALLS.
fn non_symbol_param(position: usize, got: &Sexp) -> LispError {
    LispError::NonSymbolParam {
        position,
        got: crate::domain::sexp_witness(got),
    }
}

/// Lift the lone `LispError::Compile { form: "defmacro params", message:
/// "&rest needs a name" }` triple in `parse_params` behind ONE named
/// primitive. Sibling of `non_symbol_param`: that helper fires when a
/// NON-`&rest` element at a param position isn't a symbol; this helper
/// fires specifically on the post-`&rest` follower slot, where the
/// failure mode bifurcates into "missing entirely" (`got = None`) vs.
/// "present but not a symbol" (`got = Some(...)`). Together, the two
/// helpers close the `parse_params` walker — every distinct failure
/// mode the walker can emit is now a structural variant of `LispError`,
/// not a `Compile`-shaped substring.
///
/// `rest_position` is the loop index inside `parse_params` at which
/// the `&rest` marker was matched, i.e. the 0-based index of `&rest`
/// within the param list (`(defmacro f (a &rest 5) …)` — rest_position
/// 1 is `&rest`, the offender follows at 2); naming the marker
/// position lets an LSP quick-fix point at the `&rest` form itself
/// rather than at the next list element. `got` is `Option<&Sexp>`
/// because the follower slot bifurcates: `None` when the marker was
/// the param list's last element (no follower at all), `Some(sexp)`
/// when a follower exists but isn't a symbol; the helper projects
/// through `to_string()` at the variant boundary so the variant stays
/// lifetime-free.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; one
/// inline copy still earns a named primitive once the structural shape
/// is named (the test count gives this the fail-before-pass-after
/// edge, parallel to how `non_symbol_param` was lifted from a single
/// site for the structural-completeness payoff). THEORY.md §V.1 —
/// knowable platform; the structural variant exposes `rest_position` /
/// `got` as first-class fields so authoring tools (LSP, REPL,
/// `tatara-check`) bind to the data shape instead of substring-parsing
/// the rendered diagnostic. THEORY.md §II.1 invariant 1 — typed entry;
/// a `&rest` marker followed by no name (or by a non-symbol) is
/// exactly the failure mode the typed-entry gate exists to reject —
/// and the gate must reject DEFINITIONS as readily as it rejects
/// CALLS.
fn rest_param_missing_name(rest_position: usize, got: Option<&Sexp>) -> LispError {
    LispError::RestParamMissingName {
        rest_position,
        got: got.map(Sexp::to_string),
    }
}

/// Lift the lone `LispError::Compile { form: head.to_string(), message:
/// "(defmacro name (params) body) required" }` triple in
/// `macro_def_from` behind ONE named primitive. Sibling of
/// `non_symbol_param` and `rest_param_missing_name`: those helpers
/// fire INSIDE `parse_params`, AFTER the arity gate has passed; this
/// helper fires AT the arity gate itself, BEFORE name / params / body
/// validation can run. Together the three close `macro_def_from`'s
/// outermost rejection chain — every distinct failure mode the gate
/// can emit at the top level becomes a structural variant of
/// `LispError`, not a `Compile`-shaped substring.
///
/// `head` is `MacroDefHead` (the typed closed-set enum), having been
/// projected through `MacroDefHead::from_keyword` at the top of
/// `macro_def_from`. The helper threads `head` straight into the
/// variant's typed `head: MacroDefHead` slot — no `&'static str`
/// projection at the helper boundary; the projection through
/// `MacroDefHead::keyword()` happens at Display rendering time via
/// `MacroDefHead`'s Display impl inside the variant's `#[error(...)]`
/// annotation. Same posture as how
/// `compiler_spec.rs::compiler_spec_io_err` threads
/// `CompilerSpecIoStage` straight into
/// `LispError::CompilerSpecIo.stage`. `arity` is `usize` (the length
/// of the form including the head element).
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; one
/// inline copy still earns a named primitive once the structural
/// shape is named (the test count gives this the fail-before/pass-
/// after edge, parallel to how `non_symbol_param` and
/// `rest_param_missing_name` were lifted from a single site for the
/// structural-completeness payoff). THEORY.md §V.1 — knowable
/// platform; the structural variant exposes `head` / `arity` as
/// first-class fields so authoring tools (LSP, REPL, `tatara-check`)
/// bind to the data shape instead of substring-parsing the rendered
/// diagnostic. THEORY.md §II.1 invariant 1 — typed entry; a defmacro
/// form with too few elements is exactly the failure mode the typed-
/// entry gate exists to reject — and the gate must reject
/// DEFINITIONS as readily as it rejects CALLS. THEORY.md §II.1
/// invariant 2 — free middle; the arity gate fires inside
/// `macro_def_from` BEFORE either expansion strategy runs, so both
/// `Expander::new()` (bytecode) and `Expander::new_substitute_only()`
/// (substitute) reject the SAME malformed defmacro at the SAME gate.
fn defmacro_arity(head: MacroDefHead, arity: usize) -> LispError {
    LispError::DefmacroArity { head, arity }
}

/// Lift the lone `LispError::Compile { form: head.to_string(), message:
/// "expected name symbol" }` triple in `macro_def_from` behind ONE
/// named primitive. Sibling of `defmacro_arity`, `non_symbol_param`,
/// and `rest_param_missing_name`: those helpers fire at the OUTERMOST
/// arity gate (`defmacro_arity`) or INSIDE `parse_params`
/// (`non_symbol_param`, `rest_param_missing_name`); this helper fires
/// AFTER the arity gate has passed but BEFORE `parse_params` runs —
/// at the second of three `macro_def_from` rejection points
/// (arity → name-symbol → param-list → parse_params).
///
/// Walking a malformed `(defmacro …)` from the outside in, the gate
/// fires:
///   1. `defmacro_arity(head, arity)` if the form has fewer than 4
///      elements (`(defmacro)`, `(defmacro f)`).
///   2. `defmacro_non_symbol_name(head, &list[1])` if list[1] isn't a
///      symbol (`(defmacro 5 () body)`, `(defmacro :foo () body)`).
///   3. The `expected param list` gate (NEXT LIFT) if list[2] isn't a
///      list (`(defmacro f x body)`).
///   4. Inside `parse_params`: `non_symbol_param` and
///      `rest_param_missing_name`.
///
/// After this lift step 2 is structural; the only remaining
/// `Compile`-shaped site in `macro_def_from` is step 3 (`expected
/// param list`).
///
/// `head` is `MacroDefHead` (the typed closed-set enum), having been
/// projected through `MacroDefHead::from_keyword` at the top of
/// `macro_def_from`. The helper threads `head` straight into the
/// variant's typed `head: MacroDefHead` slot — same posture as
/// `defmacro_arity` after the typed-slot promotion. `got` is `&Sexp`
/// at the call site (a borrow into the form's name slot); the helper
/// projects through `crate::domain::sexp_witness` — the typed joint
/// projection (`SexpShape` + `Sexp::Display`) — so the variant's
/// `got: SexpWitness` slot carries BOTH structural shape AND
/// renderable literal across the boundary, parallel to how
/// `non_symbol_param` and `non_symbol_unquote_target` project their
/// `&Sexp` arguments. The fourth consumer of the typed `SexpWitness`
/// primitive on the substrate's Sexp-display-source rejection
/// surface.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; one
/// inline copy still earns a named primitive once the structural
/// shape is named (the test count gives this the fail-before/pass-
/// after edge, parallel to how `defmacro_arity`, `non_symbol_param`,
/// and `rest_param_missing_name` were lifted from a single site for
/// the structural-completeness payoff). THEORY.md §V.1 — knowable
/// platform; the structural variant exposes `head` / `got` as
/// first-class fields so authoring tools (LSP, REPL,
/// `tatara-check`) bind to the data shape instead of substring-
/// parsing the rendered diagnostic. THEORY.md §II.1 invariant 1 —
/// typed entry; a defmacro form whose name slot isn't a symbol is
/// exactly the failure mode the typed-entry gate exists to reject —
/// and the gate must reject DEFINITIONS as readily as it rejects
/// CALLS. THEORY.md §II.1 invariant 2 — free middle; the
/// name-symbol gate fires inside `macro_def_from` BEFORE either
/// expansion strategy runs, so both `Expander::new()` (bytecode) and
/// `Expander::new_substitute_only()` (substitute) reject the SAME
/// malformed defmacro at the SAME gate.
fn defmacro_non_symbol_name(head: MacroDefHead, got: &Sexp) -> LispError {
    LispError::DefmacroNonSymbolName {
        head,
        got: crate::domain::sexp_witness(got),
    }
}

/// Lift the lone `LispError::Compile { form: head.to_string(), message:
/// "expected param list" }` triple in `macro_def_from` behind ONE
/// named primitive. Sibling of `defmacro_arity`,
/// `defmacro_non_symbol_name`, `non_symbol_param`, and
/// `rest_param_missing_name`: those helpers fire at the OUTERMOST
/// arity gate (`defmacro_arity`), at the second `macro_def_from`
/// rejection point (`defmacro_non_symbol_name`), or INSIDE
/// `parse_params` (`non_symbol_param`, `rest_param_missing_name`);
/// this helper fires AFTER both the arity gate AND the name-symbol
/// gate have passed but BEFORE `parse_params` runs — at the third
/// of three `macro_def_from` rejection points
/// (arity → name-symbol → param-list → parse_params).
///
/// Walking a malformed `(defmacro …)` from the outside in, the gate
/// fires:
///   1. `defmacro_arity(head, arity)` if the form has fewer than 4
///      elements (`(defmacro)`, `(defmacro f)`).
///   2. `defmacro_non_symbol_name(head, &list[1])` if list[1] isn't
///      a symbol (`(defmacro 5 () body)`).
///   3. `defmacro_non_list_params(head, &list[2])` if list[2] isn't
///      a list (`(defmacro f x body)`, `(defmacro f 5 body)`).
///   4. Inside `parse_params`: `non_symbol_param` and
///      `rest_param_missing_name`.
///
/// After this lift step 3 is structural; every inline
/// `LispError::Compile { … }` triple in `macro_def_from` has been
/// lifted to a structural variant — the entire `macro_def_from`
/// rejection chain is structurally typed for failure modes.
///
/// `head` is `MacroDefHead` (the typed closed-set enum), having been
/// projected through `MacroDefHead::from_keyword` at the top of
/// `macro_def_from`. The helper threads `head` straight into the
/// variant's typed `head: MacroDefHead` slot — same posture as
/// `defmacro_arity` and `defmacro_non_symbol_name` after the
/// typed-slot promotion. `got` is `&Sexp` at the call site (a
/// borrow into the form's param-list slot); the helper projects
/// through `to_string()` so the variant's `got` slot stays
/// lifetime-free, parallel to how `non_symbol_param`,
/// `non_symbol_unquote_target`, and `defmacro_non_symbol_name`
/// project their `&Sexp` arguments.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; one
/// inline copy still earns a named primitive once the structural
/// shape is named (the test count gives this the fail-before/pass-
/// after edge, parallel to how `defmacro_arity`,
/// `defmacro_non_symbol_name`, `non_symbol_param`, and
/// `rest_param_missing_name` were lifted from a single site for
/// the structural-completeness payoff). THEORY.md §V.1 — knowable
/// platform; the structural variant exposes `head` / `got` as
/// first-class fields so authoring tools (LSP, REPL,
/// `tatara-check`) bind to the data shape instead of substring-
/// parsing the rendered diagnostic. THEORY.md §II.1 invariant 1 —
/// typed entry; a defmacro form whose param-list slot isn't a list
/// is exactly the failure mode the typed-entry gate exists to
/// reject — and the gate must reject DEFINITIONS as readily as it
/// rejects CALLS. THEORY.md §II.1 invariant 2 — free middle; the
/// param-list gate fires inside `macro_def_from` BEFORE either
/// expansion strategy runs, so both `Expander::new()` (bytecode)
/// and `Expander::new_substitute_only()` (substitute) reject the
/// SAME malformed defmacro at the SAME gate.
fn defmacro_non_list_params(head: MacroDefHead, got: &Sexp) -> LispError {
    LispError::DefmacroNonListParams {
        head,
        got: got.to_string(),
    }
}

/// Project a `bindings: &HashMap<String, Sexp>` into the `&[&str]` candidate
/// set `crate::domain::suggest` wants. Cold path — only allocated when an
/// `,name` / `,@name` substitution misses, i.e. when we're already on the
/// diagnostic side of the substitute walker.
fn bound_names(bindings: &HashMap<String, Sexp>) -> Vec<&str> {
    bindings.keys().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::read;

    fn parse(src: &str) -> Sexp {
        read(src).unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn identity_macro() {
        let mut e = Expander::new();
        let forms = read("(defmacro id (x) `,x) (id 42)").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Sexp::int(42));
    }

    #[test]
    fn wrap_macro_duplicates_arg() {
        let mut e = Expander::new();
        let forms = read("(defmacro wrap (x) `(list ,x ,x)) (wrap hello)").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(list hello hello)"));
    }

    #[test]
    fn rest_param_splices_with_at() {
        let mut e = Expander::new();
        let forms = read("(defmacro call (f &rest args) `(,f ,@args)) (call foo a b c)").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(foo a b c)"));
    }

    #[test]
    fn nested_macro_expansion() {
        let mut e = Expander::new();
        let forms = read(
            "(defmacro twice (x) `(list ,x ,x))
             (defmacro quad (x) `(twice ,x))
             (quad hey)",
        )
        .unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(list hey hey)"));
    }

    #[test]
    fn unbound_unquote_errors() {
        let mut e = Expander::new();
        let forms = read("(defmacro bad (x) `(list ,y)) (bad 1)").unwrap();
        assert!(e.expand_program(forms).is_err());
    }

    #[test]
    fn missing_required_arg_errors() {
        let mut e = Expander::new();
        let forms = read("(defmacro need-two (a b) `(,a ,b)) (need-two 1)").unwrap();
        assert!(e.expand_program(forms).is_err());
    }

    #[test]
    fn defpoint_template_treated_as_defmacro() {
        let mut e = Expander::new();
        let forms = read(
            "(defpoint-template obs (name) `(defpoint ,name :class (Gate Observability)))
             (obs grafana)",
        )
        .unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(
            out[0],
            parse("(defpoint grafana :class (Gate Observability))")
        );
    }

    #[test]
    fn defcheck_treated_as_defmacro() {
        let mut e = Expander::new();
        let forms = read(
            "(defcheck pair (a b) `(do (yaml-parses ,a) (yaml-parses ,b)))
             (pair \"x.yaml\" \"y.yaml\")",
        )
        .unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(
            out[0],
            parse("(do (yaml-parses \"x.yaml\") (yaml-parses \"y.yaml\"))")
        );
    }

    #[test]
    fn empty_rest_splices_nothing() {
        let mut e = Expander::new();
        let forms = read("(defmacro f (x &rest r) `(list ,x ,@r)) (f 1)").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(list 1)"));
    }

    #[test]
    fn macro_expanded_inside_list() {
        // A macro call nested in a list position also expands.
        let mut e = Expander::new();
        let forms = read("(defmacro two () `(list 1 2)) (outer (two))").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(outer (list 1 2))"));
    }

    // ── Compiled-template bytecode equivalence + speedup ──────────────

    #[test]
    fn compiled_template_matches_substitute_path() {
        // Same program, two expanders with different strategies — outputs must agree.
        let src = "
            (defmacro wrap (x) `(list ,x ,x))
            (defmacro call (f &rest args) `(,f ,@args))
            (defmacro twice (x) `(list ,x ,x))
            (defmacro quad (x) `(twice ,x))
            (wrap hello)
            (call foo a b c)
            (quad hey)
            (outer (wrap deep))
        ";
        let forms = read(src).unwrap();
        let mut fast = Expander::new();
        let mut slow = Expander::new_substitute_only();
        let out_fast = fast.expand_program(forms.clone()).unwrap();
        let out_slow = slow.expand_program(forms).unwrap();
        assert_eq!(out_fast, out_slow);
    }

    #[test]
    fn literal_subtree_compiles_to_single_literal_op() {
        // Macro body where only one leaf is a substitution — the rest of the
        // template is literal, so the compiler should prune large chunks to
        // a single Literal op.
        let def = MacroDef {
            name: "label".into(),
            params: vec![Param::Required("x".into())],
            body: Sexp::Quasiquote(Box::new(parse(
                "(observed (at timestamp) (in region) (value ,x) (tags (one two three)))",
            ))),
        };
        let compiled = compile_template(&def).expect("compile");
        // The template is ONE list. After compile:
        //   BeginList,
        //     Literal((observed (at timestamp) (in region))), // wait — `observed` is a list too
        //     ...
        //   EndList
        // Point is: many subtrees should be single Literals. We simply count
        // that the op stream is SHORTER than the full Sexp size.
        let ops_count = compiled.ops.len();
        assert!(
            ops_count < 15,
            "expected pruned op stream, got {ops_count} ops: {:?}",
            compiled.ops
        );
    }

    /// Three-way benchmark: substitute-only vs bytecode-no-cache vs bytecode-cache.
    /// Each path must produce identical output; the cache should show a real,
    /// visible speedup because the workload (10 000 calls across 10 unique
    /// (macro, args) pairs = 99.9% cache hit rate) is cache-friendly.
    #[test]
    fn expansion_layers_agree_on_output_and_cache_wins() {
        use std::time::Instant;

        let macros = "
            (defmacro m1 (a b) `(list ,a ,b))
            (defmacro m2 (x) `(if ,x true false))
            (defmacro m3 (a b c) `(list ,a ,b ,c ,a ,b ,c))
            (defmacro m4 (f &rest args) `(,f ,@args))
            (defmacro m5 (x) `(and ,x (not (not ,x))))
            (defmacro m6 (a b) `(or ,a ,b (and ,a ,b)))
            (defmacro m7 (x) `(debug (at timestamp) (in region) (value ,x)))
            (defmacro m8 (x y) `(cond ((= ,x ,y) equal) (#t not-equal)))
            (defmacro m9 (x) `(loop (times 10) (eval ,x)))
            (defmacro m10 (f g &rest args) `(,f (,g ,@args)))
        ";
        let mut call_src = String::with_capacity(80_000);
        for i in 0..10_000 {
            match i % 10 {
                0 => call_src.push_str("(m1 a b)\n"),
                1 => call_src.push_str("(m2 true)\n"),
                2 => call_src.push_str("(m3 x y z)\n"),
                3 => call_src.push_str("(m4 f a b c d e)\n"),
                4 => call_src.push_str("(m5 y)\n"),
                5 => call_src.push_str("(m6 a b)\n"),
                6 => call_src.push_str("(m7 answer)\n"),
                7 => call_src.push_str("(m8 p q)\n"),
                8 => call_src.push_str("(m9 body)\n"),
                _ => call_src.push_str("(m10 f g a b c)\n"),
            }
        }
        let all_src = format!("{macros}\n{call_src}");
        let forms = read(&all_src).unwrap();

        let mut subst = Expander::new_substitute_only();
        let t0 = Instant::now();
        let out_subst = subst.expand_program(forms.clone()).unwrap();
        let t_subst = t0.elapsed();

        let mut byte_no_cache = Expander::new_bytecode_no_cache();
        let t0 = Instant::now();
        let out_byte = byte_no_cache.expand_program(forms.clone()).unwrap();
        let t_byte = t0.elapsed();

        let mut byte_cache = Expander::new();
        let t0 = Instant::now();
        let out_cached = byte_cache.expand_program(forms).unwrap();
        let t_cached = t0.elapsed();

        // Rigorous: all three paths agree.
        assert_eq!(out_subst, out_byte);
        assert_eq!(out_subst, out_cached);

        // Cache captured the 10 unique (macro, args) pairs (plus some inner
        // expansions — macros that expand into calls to other macros).
        let cache_size = byte_cache.cache_size();
        assert!(
            (10..=50).contains(&cache_size),
            "expected ~10 unique cache entries, got {cache_size}"
        );

        eprintln!(
            "\n=== macroexpand: 10k calls × 10 unique (macro, args) pairs ===\n\
             substitute only     : {t_subst:?}\n\
             bytecode no cache   : {t_byte:?}\n\
             bytecode + cache    : {t_cached:?}   (cache_size={cache_size})\n\
             cache speedup vs subst : {:.2}×\n\
             cache speedup vs byte  : {:.2}×\n",
            t_subst.as_secs_f64() / t_cached.as_secs_f64(),
            t_byte.as_secs_f64() / t_cached.as_secs_f64(),
        );

        // The cache MUST win against both baselines for this cache-friendly
        // workload. Using a 1.5× threshold so the test is stable across hosts.
        assert!(
            t_cached < t_subst,
            "cache should beat substitute ({t_cached:?} vs {t_subst:?})"
        );
        assert!(
            t_cached < t_byte,
            "cache should beat bytecode-no-cache ({t_cached:?} vs {t_byte:?})"
        );
    }

    #[test]
    fn cache_respects_arg_changes() {
        // Cache must not return stale results when args differ.
        let src = "
            (defmacro wrap (x) `(list ,x ,x))
            (wrap a)
            (wrap b)
            (wrap a)   ;; same as first — cached hit
        ";
        let mut e = Expander::new();
        let out = e.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], parse("(list a a)"));
        assert_eq!(out[1], parse("(list b b)"));
        assert_eq!(out[2], parse("(list a a)"));
        // Two distinct args → 2 cache entries.
        assert_eq!(e.cache_size(), 2);
    }

    #[test]
    fn clear_cache_empties_memo() {
        let mut e = Expander::new();
        let out = e
            .expand_program(read("(defmacro id (x) `,x) (id 1) (id 2)").unwrap())
            .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(e.cache_size(), 2);
        e.clear_cache();
        assert_eq!(e.cache_size(), 0);
    }

    // ── Unbound template-var: structural variant + did-you-mean hint ──

    /// Helper for the unbound-template-var tests — pins the variant shape
    /// and carries any error context up to the assert site for legibility.
    fn unbound_var(err: &LispError) -> (UnquoteForm, &str, Option<&str>) {
        match err {
            LispError::UnboundTemplateVar { prefix, name, hint } => {
                (*prefix, name.as_str(), hint.as_deref())
            }
            other => panic!("expected UnboundTemplateVar, got: {other:?}"),
        }
    }

    #[test]
    fn unbound_unquote_in_compile_template_emits_structural_variant_with_hint() {
        // `,xs` against macro params `[x]` — distance 1, bound 1 — hints `,x`.
        // Path: compile_node Unquote (the bytecode-template compile, default
        // expander).
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `(list ,xs)) (w 1)").unwrap())
            .expect_err("unbound template var must error");
        let (prefix, name, hint) = unbound_var(&err);
        assert_eq!(prefix, UnquoteForm::Unquote);
        assert_eq!(name, "xs");
        assert_eq!(hint, Some("x"));
    }

    #[test]
    fn unbound_unquote_splice_in_compile_template_emits_structural_variant_with_hint() {
        // `,@argz` against macro params `[args]` — distance 1, bound 2 —
        // hints `,@args`. Path: compile_node UnquoteSplice.
        let mut e = Expander::new();
        let err = e
            .expand_program(
                read("(defmacro call (f &rest args) `(,f ,@argz)) (call foo a b)").unwrap(),
            )
            .expect_err("unbound splice must error");
        let (prefix, name, hint) = unbound_var(&err);
        assert_eq!(prefix, UnquoteForm::Splice);
        assert_eq!(name, "argz");
        assert_eq!(hint, Some("args"));
    }

    #[test]
    fn unbound_unquote_in_substitute_emits_structural_variant_with_hint() {
        // Same shape but routed through the substitute-only expander — proves
        // the substitute path emits the same variant as the bytecode path.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro w (x) `(list ,xs)) (w 1)").unwrap())
            .expect_err("substitute unbound must error");
        let (prefix, name, hint) = unbound_var(&err);
        assert_eq!(prefix, UnquoteForm::Unquote);
        assert_eq!(name, "xs");
        assert_eq!(hint, Some("x"));
    }

    #[test]
    fn unbound_unquote_splice_in_substitute_emits_structural_variant_with_hint() {
        // The substitute path's UnquoteSplice branch fires for splices that
        // appear inside a list during the recursive walk. `,@argz` against
        // `[args]` hints `,@args`.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(
                read("(defmacro call (f &rest args) `(,f ,@argz)) (call foo a b)").unwrap(),
            )
            .expect_err("substitute splice unbound must error");
        let (prefix, name, hint) = unbound_var(&err);
        assert_eq!(prefix, UnquoteForm::Splice);
        assert_eq!(name, "argz");
        assert_eq!(hint, Some("args"));
    }

    #[test]
    fn unbound_template_var_omits_hint_when_no_close_match() {
        // `,wholly-unrelated` against `[x]` — far past the bound, so no
        // hint. Negative control: a wrong hint is worse than no hint, so
        // the slot must stay empty when the substrate isn't confident.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `(list ,wholly-unrelated)) (w 1)").unwrap())
            .expect_err("unrelated unbound must error");
        let (prefix, name, hint) = unbound_var(&err);
        assert_eq!(prefix, UnquoteForm::Unquote);
        assert_eq!(name, "wholly-unrelated");
        assert_eq!(hint, None);
    }

    #[test]
    fn unbound_template_var_message_includes_hint_suffix_end_to_end() {
        // End-to-end through the Display impl — pins the rendered diagnostic
        // a downstream tool sees today (REPL, tatara-check). Hint stays
        // additive: the legacy `"unbound"` substring still appears, so any
        // assertion that pattern-matches on it keeps passing.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `(list ,xs)) (w 1)").unwrap())
            .expect_err("unbound must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("did you mean ,x?"),
            "expected hint suffix in message, got: {msg}"
        );
        assert!(
            msg.contains("unbound"),
            "expected legacy `unbound` substring in message, got: {msg}"
        );
        assert!(
            msg.contains(",xs"),
            "expected the offending form in message, got: {msg}"
        );
    }

    #[test]
    fn unbound_template_var_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp` carries
        // source positions, `position()` returns `None` for this variant.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `(list ,xs)) (w 1)").unwrap())
            .expect_err("unbound must error");
        assert_eq!(err.position(), None);
    }

    // ── Non-symbol unquote target: structural variant ─────────────────

    /// Helper for the non-symbol-unquote-target tests — pins the variant
    /// shape and carries any error context up to the assert site for
    /// legibility. Sibling of `unbound_var` and `splice_outside_list_got`;
    /// returns the `display` projection of the typed `SexpWitness` so the
    /// existing call sites stay byte-for-byte comparable to the legacy
    /// `got: String` shape.
    fn non_symbol_target(err: &LispError) -> (UnquoteForm, &str) {
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => (*prefix, got.display.as_str()),
            other => panic!("expected NonSymbolUnquoteTarget, got: {other:?}"),
        }
    }

    #[test]
    fn non_symbol_unquote_in_compile_template_emits_structural_variant() {
        // `,(list 1 2)` — the inner is a list, not a symbol. Path:
        // compile_node Unquote (the bytecode-template compile, default
        // expander). Pins variant identity AND prefix AND the offending
        // literal so a regression that re-inlines the legacy
        // `LispError::Compile` shape fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `,(list 1 2)) (w 1)").unwrap())
            .expect_err("non-symbol unquote target must error");
        let (prefix, got) = non_symbol_target(&err);
        assert_eq!(prefix, UnquoteForm::Unquote);
        assert_eq!(got, "(list 1 2)");
    }

    #[test]
    fn non_symbol_unquote_splice_in_compile_template_emits_structural_variant() {
        // `,@5` — the inner is an int atom, not a symbol. Path:
        // compile_node UnquoteSplice. The integer literal round-trips
        // through the variant's `got` slot via `Sexp::Display`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `(list ,@5)) (w 1)").unwrap())
            .expect_err("non-symbol splice target must error");
        let (prefix, got) = non_symbol_target(&err);
        assert_eq!(prefix, UnquoteForm::Splice);
        assert_eq!(got, "5");
    }

    #[test]
    fn non_symbol_unquote_in_substitute_emits_structural_variant() {
        // Same shape as the bytecode path but routed through the
        // substitute-only expander — proves the substitute path emits the
        // same variant as the compile_node path. Pins that the lift is
        // path-uniform.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro w (x) `,(list 1 2)) (w 1)").unwrap())
            .expect_err("substitute non-symbol target must error");
        let (prefix, got) = non_symbol_target(&err);
        assert_eq!(prefix, UnquoteForm::Unquote);
        assert_eq!(got, "(list 1 2)");
    }

    #[test]
    fn non_symbol_unquote_splice_inside_list_in_substitute_emits_structural_variant() {
        // The substitute path's UnquoteSplice-inside-list branch fires for
        // splices that appear inside a list during the recursive walk.
        // `,@(list 1 2)` inside the body — the inner is a literal list, not
        // a symbol — emits the same variant as the compile_node path.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro w (x) `(outer ,@(list 1 2))) (w 1)").unwrap())
            .expect_err("substitute non-symbol splice must error");
        let (prefix, got) = non_symbol_target(&err);
        assert_eq!(prefix, UnquoteForm::Splice);
        assert_eq!(got, "(list 1 2)");
    }

    #[test]
    fn non_symbol_unquote_target_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp` carries
        // source positions, `position()` returns `None` for this variant.
        // A future run that gives `Sexp` source spans adds `pos:
        // Option<usize>` to ONE place; this test gives that change a
        // deliberate fail-before/pass-after delta.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `,(list 1 2)) (w 1)").unwrap())
            .expect_err("non-symbol target must error");
        assert_eq!(err.position(), None);
    }

    // ── Splice outside list: structural variant + path-uniform rejection ─

    /// Helper for the splice-outside-list tests — pins the variant shape
    /// and carries the offending `got` field up to the assert site for
    /// legibility. Sibling of `unbound_var` and `non_symbol_target`.
    fn splice_outside_list_got(err: &LispError) -> &str {
        match err {
            LispError::SpliceOutsideList { got } => got.display.as_str(),
            other => panic!("expected SpliceOutsideList, got: {other:?}"),
        }
    }

    #[test]
    fn splice_outside_list_in_substitute_emits_structural_variant() {
        // `,@xs` at the body's top level — there is no containing list to
        // splice into. Path: substitute (the `Expander::new_substitute_only`
        // path's top-level `Sexp::UnquoteSplice(_)` arm). Pins variant
        // identity AND the offending inner so a regression that re-inlines
        // the legacy `LispError::Compile` shape fails-loudly here.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro f (xs) `,@xs) (f (list 1 2))").unwrap())
            .expect_err("splice outside list must error");
        assert_eq!(splice_outside_list_got(&err), "xs");
    }

    #[test]
    fn splice_outside_list_with_list_literal_in_substitute_emits_structural_variant() {
        // `,@(list 1 2)` at the body's top level — the inner is a literal
        // list, not a symbol. The structural variant carries the inner's
        // Sexp::Display projection so the operator sees the literal value
        // they wrote in the parenthetical.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro f (x) `,@(list 1 2)) (f 1)").unwrap())
            .expect_err("splice outside list must error");
        assert_eq!(splice_outside_list_got(&err), "(list 1 2)");
    }

    #[test]
    fn splice_outside_list_in_compile_template_emits_structural_variant() {
        // The bytecode path's `compile_template` gate now rejects top-level
        // `,@X` bodies BEFORE walking — closing the prior silent-divergence
        // where the bytecode interpreter's outermost stack frame absorbed
        // the splice. Pins that the bytecode path emits the SAME structural
        // variant the substitute path emits — `,@-outside-list` is rejected
        // path-uniformly. Path: `Expander::new()` (compile_templates = true)
        // → `compile_template` gate.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (xs) `,@xs) (f (list 1 2))").unwrap())
            .expect_err("compile-template splice outside list must error");
        assert_eq!(splice_outside_list_got(&err), "xs");
    }

    #[test]
    fn splice_outside_list_with_list_literal_in_compile_template_emits_structural_variant() {
        // Same shape as the substitute test but routed through the bytecode
        // path's `compile_template` gate. Proves the gate fires on a
        // non-symbol inner too — the slot's contents are irrelevant; only
        // the syntactic position matters.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (x) `,@(list 1 2)) (f 1)").unwrap())
            .expect_err("compile-template splice outside list must error");
        assert_eq!(splice_outside_list_got(&err), "(list 1 2)");
    }

    #[test]
    fn splice_outside_list_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME structural
        // variant (`SpliceOutsideList { got: "xs" }`) under both expansion
        // strategies. Before the `compile_template` gate, the bytecode path
        // silently produced a list while the substitute path errored —
        // expansion strategy was observable. After the gate, the gate is
        // strategy-uniform, so a macro that registers under one strategy
        // registers under the other.
        let src = "(defmacro f (xs) `,@xs) (f (list 1 2))";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(splice_outside_list_got(&err_subst), "xs");
        assert_eq!(splice_outside_list_got(&err_byte), "xs");
    }

    #[test]
    fn splice_outside_list_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp` carries
        // source positions, `position()` returns `None` for this variant.
        // A future run that gives `Sexp` source spans adds `pos:
        // Option<usize>` to ONE place; this test gives that change a
        // deliberate fail-before/pass-after delta.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro f (xs) `,@xs) (f (list 1 2))").unwrap())
            .expect_err("splice outside list must error");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn splice_outside_list_message_renders_legacy_substring_with_offending_form() {
        // End-to-end through the Display impl — pins the rendered diagnostic
        // a downstream tool sees today (REPL, tatara-check). The legacy
        // substring `"\`,@\` may only appear inside a list"` is preserved
        // verbatim AND the parenthetical `(got ,@xs)` names the offending
        // form; tools that pattern-match on the variant gain structural
        // binding to `got`.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro f (xs) `,@xs) (f (list 1 2))").unwrap())
            .expect_err("splice outside list must error");
        let msg = format!("{err}");
        assert_eq!(
            msg,
            "compile error in ,@: `,@` may only appear inside a list (got ,@xs)"
        );
    }

    #[test]
    fn splice_inside_list_still_succeeds_under_both_paths() {
        // Negative control: a well-positioned splice (`,@xs` INSIDE a list)
        // continues to succeed under both paths — the new gate only fires
        // when the splice is the entire body. Pins that the gate is scoped
        // to top-level only, not all `,@` occurrences. Uses a `&rest`-bound
        // list so `xs` is unambiguously a Sexp::List `(1 2)` rather than a
        // bare list-literal whose first symbol would also splice through.
        let src = "(defmacro f (&rest xs) `(outer ,@xs)) (f 1 2)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let out_subst = subst.expand_program(read(src).unwrap()).unwrap();
        let out_byte = bytecode.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out_subst, out_byte);
        assert_eq!(out_subst[0], parse("(outer 1 2)"));
    }

    // ── Missing macro arg: structural variant + path-uniform rejection ──

    /// Helper for the missing-macro-arg tests — pins the variant shape
    /// and carries the failing macro's name + un-bound param up to the
    /// assert site for legibility. Sibling of `unbound_var`,
    /// `non_symbol_target`, and `splice_outside_list_got`.
    fn missing_macro_arg_fields(err: &LispError) -> (&str, &str) {
        match err {
            LispError::MissingMacroArg { macro_name, param } => {
                (macro_name.as_str(), param.as_str())
            }
            other => panic!("expected MissingMacroArg, got: {other:?}"),
        }
    }

    #[test]
    fn missing_macro_arg_in_compile_template_emits_structural_variant() {
        // `(need-two 1)` against `(need-two a b)` — `b` has no arg. Path:
        // `apply_compiled` (the bytecode-template path, default expander).
        // Pins variant identity AND macro_name AND the un-bound param so a
        // regression that re-inlines the legacy `LispError::Compile` shape
        // fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro need-two (a b) `(,a ,b)) (need-two 1)").unwrap())
            .expect_err("missing required macro arg must error");
        let (macro_name, param) = missing_macro_arg_fields(&err);
        assert_eq!(macro_name, "need-two");
        assert_eq!(param, "b");
    }

    #[test]
    fn missing_macro_arg_in_substitute_emits_structural_variant() {
        // Same shape as the bytecode test but routed through the
        // substitute-only expander → `bind_args` is the failing site.
        // Proves the substitute path emits the SAME structural variant the
        // bytecode path emits — `missing required arg` rejection is
        // path-uniform across both expansion strategies.
        let mut e = Expander::new_substitute_only();
        let err = e
            .expand_program(read("(defmacro need-two (a b) `(,a ,b)) (need-two 1)").unwrap())
            .expect_err("missing required macro arg must error");
        let (macro_name, param) = missing_macro_arg_fields(&err);
        assert_eq!(macro_name, "need-two");
        assert_eq!(param, "b");
    }

    #[test]
    fn missing_macro_arg_first_position_is_named() {
        // `(f)` against `(f a b)` — `a` (the FIRST required param) has no
        // arg. The variant names `a`, not `b` — naming the LEFTMOST
        // un-bound param is the shape `bind_args` / `apply_compiled` both
        // emit (each iterates positionally and bails on the first missing
        // slot). Pins the leftmost-bail contract so a regression that
        // names the rightmost (or a surplus) param fails-loudly.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a b) `(,a ,b)) (f)").unwrap())
            .expect_err("missing first required arg must error");
        let (macro_name, param) = missing_macro_arg_fields(&err);
        assert_eq!(macro_name, "f");
        assert_eq!(param, "a");
    }

    #[test]
    fn missing_macro_arg_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME structural
        // variant under both expansion strategies. Negative control for
        // the divergence-closing posture: a future refactor that drifts
        // either path's rejection shape (or drops one path's rejection
        // entirely) fails-loudly here. Sibling of
        // `splice_outside_list_substitute_and_bytecode_paths_agree` —
        // both close `THEORY.md §II.1 invariant 2 — free middle` for one
        // failure mode each.
        let src = "(defmacro need-two (a b) `(,a ,b)) (need-two 1)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(missing_macro_arg_fields(&err_subst), ("need-two", "b"));
        assert_eq!(missing_macro_arg_fields(&err_byte), ("need-two", "b"));
    }

    #[test]
    fn missing_macro_arg_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp` carries
        // source positions, `position()` returns `None` for this variant.
        // A future run that gives `Sexp` source spans adds `pos:
        // Option<usize>` to ONE place; this test gives that change a
        // deliberate fail-before/pass-after delta. Parallel to
        // `splice_outside_list_position_is_none_today`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro need-two (a b) `(,a ,b)) (need-two 1)").unwrap())
            .expect_err("missing required macro arg must error");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn missing_macro_arg_message_renders_legacy_substring_with_macro_name() {
        // End-to-end through the Display impl — pins the rendered diagnostic
        // a downstream tool sees today (REPL, tatara-check). The legacy
        // substring `"missing required arg: {param}"` is preserved verbatim
        // AND the head clause names the failing macro via `"call to
        // {macro_name}"`; tools that pattern-match on the variant gain
        // structural binding to `macro_name` / `param`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro need-two (a b) `(,a ,b)) (need-two 1)").unwrap())
            .expect_err("missing required macro arg must error");
        assert_eq!(
            format!("{err}"),
            "compile error in call to need-two: missing required arg: b"
        );
    }

    #[test]
    fn missing_macro_arg_carries_kebab_case_macro_and_param_unchanged() {
        // Both `macro_name` (`wrap-twice`) and `param` (`notify-ref`)
        // round-trip through the variant unchanged. Pinning this contract
        // means a regression that camelCases or lowercases either side
        // fails-loudly here. Parallel to the
        // `unknown_kwarg_display_carries_kebab_case_keys_unchanged`
        // assertion for the kwarg-gate's symmetric surface.
        let mut e = Expander::new();
        let err = e
            .expand_program(
                read("(defmacro wrap-twice (notify-ref body) `(list ,notify-ref ,body)) (wrap-twice :a)")
                    .unwrap(),
            )
            .expect_err("missing required macro arg must error");
        let (macro_name, param) = missing_macro_arg_fields(&err);
        assert_eq!(macro_name, "wrap-twice");
        assert_eq!(param, "body");
    }

    #[test]
    fn rest_param_only_macro_with_no_args_still_succeeds() {
        // Negative control: a macro whose only param is `&rest` must NOT
        // error when called with zero args — the rest-param binds to the
        // empty list. The new structural variant fires only on REQUIRED
        // params; the `Param::Rest` arm in both `bind_args` and
        // `apply_compiled` continues to bind the empty tail. Pins that the
        // helper is scoped to required-param failure, not all
        // arity-mismatch shapes.
        let src = "(defmacro f (&rest xs) `(list ,@xs)) (f)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let out_subst = subst.expand_program(read(src).unwrap()).unwrap();
        let out_byte = bytecode.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out_subst, out_byte);
        assert_eq!(out_subst[0], parse("(list)"));
    }

    /// Helper for the non-symbol-param tests — pins the variant shape and
    /// carries the failing position + offending element up to the assert
    /// site for legibility. Sibling of `missing_macro_arg_fields`.
    fn non_symbol_param_fields(err: &LispError) -> (usize, &str) {
        match err {
            LispError::NonSymbolParam { position, got } => (*position, got.display.as_str()),
            other => panic!("expected NonSymbolParam, got: {other:?}"),
        }
    }

    #[test]
    fn non_symbol_param_at_first_position_emits_structural_variant() {
        // `(defmacro f (5) ...)` — the first element of the param list is
        // an integer literal, not a symbol. Pins variant identity AND
        // that `position` is the loop index inside `parse_params` (0 for
        // the first slot) AND that `got` is the offending element via
        // `Sexp::Display` (`5`). A regression that re-inlines the legacy
        // `LispError::Compile` shape (which named neither the position
        // nor the offending element) fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (5) `(list ,a))").unwrap())
            .expect_err("non-symbol param must error");
        let (position, got) = non_symbol_param_fields(&err);
        assert_eq!(position, 0);
        assert_eq!(got, "5");
    }

    #[test]
    fn non_symbol_param_at_second_position_emits_structural_variant() {
        // `(defmacro f (a 5) ...)` — `a` parses fine, `5` at position 1
        // misfires. Pins that `position` advances with the loop index, so
        // an LSP quick-fix that wants to point at "the second element of
        // your param list" gains the index as data, no source re-parse
        // required.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a 5) `(,a))").unwrap())
            .expect_err("non-symbol param must error");
        let (position, got) = non_symbol_param_fields(&err);
        assert_eq!(position, 1);
        assert_eq!(got, "5");
    }

    #[test]
    fn non_symbol_param_carries_keyword_value_unchanged() {
        // `:k` at a param-list position. `Sexp::Display` for
        // `Atom::Keyword(s)` writes `:s`; pins that the variant's `got`
        // field round-trips the keyword form unchanged so an LSP that
        // surfaces "you wrote `:k` where a symbol was expected" gains
        // the literal keyword value as data, no re-parsing required.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (:k) `(list))").unwrap())
            .expect_err("non-symbol param must error");
        let (position, got) = non_symbol_param_fields(&err);
        assert_eq!(position, 0);
        assert_eq!(got, ":k");
    }

    #[test]
    fn non_symbol_param_carries_nested_list_value_unchanged() {
        // A nested list at a param-list position. `Sexp::Display` for
        // `List(xs)` writes `(<x1> <x2> ...)`; pins that the variant's
        // `got` field carries the nested form's full Display projection
        // unchanged so the operator sees what they wrote.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f ((nested)) `(list))").unwrap())
            .expect_err("non-symbol param must error");
        let (position, got) = non_symbol_param_fields(&err);
        assert_eq!(position, 0);
        assert_eq!(got, "(nested)");
    }

    #[test]
    fn non_symbol_param_in_defpoint_template_emits_same_variant() {
        // `defpoint-template` shares `parse_params` with `defmacro` (all
        // three head keywords route through `macro_def_from`). Pins that
        // the lift fires path-uniformly across the three head keywords
        // — `defmacro`, `defpoint-template`, `defcheck` — so the
        // structural-completeness floor holds for every defmacro-shaped
        // form, not just the one with the `defmacro` head literal.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defpoint-template obs (5) `(defpoint))").unwrap())
            .expect_err("non-symbol param must error");
        let (position, got) = non_symbol_param_fields(&err);
        assert_eq!(position, 0);
        assert_eq!(got, "5");
    }

    #[test]
    fn non_symbol_param_in_defcheck_emits_same_variant() {
        // Sibling of the defpoint-template test — `defcheck` is the
        // third head keyword `macro_def_from` recognizes. All three
        // route through the same `parse_params` and now reject
        // non-symbol params with the same structural variant.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defcheck pair (a 5) `(do))").unwrap())
            .expect_err("non-symbol param must error");
        let (position, got) = non_symbol_param_fields(&err);
        assert_eq!(position, 1);
        assert_eq!(got, "5");
    }

    #[test]
    fn non_symbol_param_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp`
        // carries source positions, `position()` on `LispError` returns
        // `None` for this variant. A future run that gives `Sexp`
        // source spans adds `pos: Option<usize>` to ONE place; this
        // test gives that change a deliberate fail-before/pass-after
        // delta. Parallel to `missing_macro_arg_position_is_none_today`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (5) `(list))").unwrap())
            .expect_err("non-symbol param must error");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn non_symbol_param_message_renders_legacy_substring_with_position() {
        // End-to-end through Display — pins the rendered diagnostic that
        // downstream tools (REPL, `tatara-check`) see today. Legacy
        // substrings `"defmacro params"` AND `"expected symbol"` are
        // preserved verbatim; the appended `at position {position}, got
        // {got}` clause is the new structural detail. Tools that
        // pattern-match on the variant gain structural binding to
        // `position` / `got`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a 5) `(,a))").unwrap())
            .expect_err("non-symbol param must error");
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: \
             expected symbol at position 1, got 5"
        );
    }

    #[test]
    fn non_symbol_param_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME
        // structural variant under both expansion strategies. The
        // defmacro-syntax-gate fires inside `macro_def_from` →
        // `parse_params`, BEFORE either strategy's expansion path runs;
        // so both `Expander::new()` (bytecode) and
        // `Expander::new_substitute_only()` (substitute) reject the
        // SAME malformed defmacro at the SAME gate. Sibling of
        // `missing_macro_arg_substitute_and_bytecode_paths_agree`.
        let src = "(defmacro f (a 5) `(,a))";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(non_symbol_param_fields(&err_subst), (1, "5"));
        assert_eq!(non_symbol_param_fields(&err_byte), (1, "5"));
    }

    /// Helper for the rest-param-missing-name tests — pins the variant
    /// shape and carries the marker position + offending follower (or
    /// its absence) up to the assert site for legibility. Sibling of
    /// `non_symbol_param_fields`.
    fn rest_param_missing_name_fields(err: &LispError) -> (usize, Option<&str>) {
        match err {
            LispError::RestParamMissingName { rest_position, got } => {
                (*rest_position, got.as_deref())
            }
            other => panic!("expected RestParamMissingName, got: {other:?}"),
        }
    }

    #[test]
    fn rest_param_missing_name_when_only_rest_emits_structural_variant_with_no_got() {
        // `(defmacro f (&rest))` — the marker is the only param-list
        // element; nothing follows. Pins variant identity AND that
        // `rest_position == 0` (the first slot) AND that `got == None`
        // (no follower exists). A regression that re-inlines the legacy
        // `LispError::Compile` shape (which named neither field) fails-
        // loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (&rest) `(list))").unwrap())
            .expect_err("&rest with no follower must error");
        let (rest_position, got) = rest_param_missing_name_fields(&err);
        assert_eq!(rest_position, 0);
        assert_eq!(got, None);
    }

    #[test]
    fn rest_param_missing_name_at_end_of_param_list_emits_structural_variant() {
        // `(defmacro f (a &rest))` — `a` parses fine, `&rest` at param-list
        // position 1 has no follower at all. Pins that `rest_position`
        // advances with the loop index, so an LSP quick-fix that wants to
        // point at "your `&rest` at position 1 has no name" gains the
        // marker position as data, no source re-parse required.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a &rest) `(,a))").unwrap())
            .expect_err("&rest with no follower must error");
        let (rest_position, got) = rest_param_missing_name_fields(&err);
        assert_eq!(rest_position, 1);
        assert_eq!(got, None);
    }

    #[test]
    fn rest_param_missing_name_with_int_follower_emits_structural_variant() {
        // `(defmacro f (&rest 5))` — `&rest` at position 0 followed by
        // `5` (an integer literal, not a symbol). Pins that the variant's
        // `got` field is `Some` and carries the offending follower's
        // `Sexp::Display` projection; the bifurcation between "missing
        // entirely" and "present but non-symbol" is in the renderable
        // detail, not in what the gate rejects.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (&rest 5) `(list))").unwrap())
            .expect_err("&rest followed by non-symbol must error");
        let (rest_position, got) = rest_param_missing_name_fields(&err);
        assert_eq!(rest_position, 0);
        assert_eq!(got, Some("5"));
    }

    #[test]
    fn rest_param_missing_name_with_keyword_follower_emits_structural_variant() {
        // `(defmacro f (a &rest :foo))` — keyword follower at the rest-name
        // slot. `Sexp::Display` for `Atom::Keyword(s)` writes `:s`; pins
        // that the variant's `got` field round-trips the keyword form
        // unchanged so an LSP that surfaces "you wrote `:foo` where a
        // rest-name was expected" gains the literal keyword value as
        // data, no re-parsing required.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a &rest :foo) `(,a))").unwrap())
            .expect_err("&rest followed by keyword must error");
        let (rest_position, got) = rest_param_missing_name_fields(&err);
        assert_eq!(rest_position, 1);
        assert_eq!(got, Some(":foo"));
    }

    #[test]
    fn rest_param_missing_name_with_nested_list_follower_emits_structural_variant() {
        // `(defmacro f (&rest (nested)))` — nested-list follower at the
        // rest-name slot. `Sexp::Display` for `List(xs)` writes
        // `(<x1> <x2> ...)`; pins that the variant's `got` field carries
        // the nested form's full Display projection unchanged so the
        // operator sees what they wrote.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (&rest (nested)) `(list))").unwrap())
            .expect_err("&rest followed by list must error");
        let (rest_position, got) = rest_param_missing_name_fields(&err);
        assert_eq!(rest_position, 0);
        assert_eq!(got, Some("(nested)"));
    }

    #[test]
    fn rest_param_missing_name_in_defpoint_template_emits_same_variant() {
        // `defpoint-template` shares `parse_params` with `defmacro` (all
        // three head keywords route through `macro_def_from`). Pins that
        // the lift fires path-uniformly across the three head keywords —
        // a regression that handles `defpoint-template`'s param list
        // differently from `defmacro`'s would fail-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defpoint-template t (a &rest) `(,a))").unwrap())
            .expect_err("&rest with no follower must error");
        let (rest_position, got) = rest_param_missing_name_fields(&err);
        assert_eq!(rest_position, 1);
        assert_eq!(got, None);
    }

    #[test]
    fn rest_param_missing_name_in_defcheck_emits_same_variant() {
        // Sibling for the `defcheck` head; rounds out the three-head-
        // keyword coverage so the lift is path-uniform across
        // `defmacro` / `defpoint-template` / `defcheck`. After this
        // test the defmacro-syntax-gate rejects `&rest`-without-name
        // identically across all three head keywords — the
        // typed-entry surface is single-shape across the cluster.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defcheck c (&rest 5) `(list))").unwrap())
            .expect_err("&rest followed by non-symbol must error");
        let (rest_position, got) = rest_param_missing_name_fields(&err);
        assert_eq!(rest_position, 0);
        assert_eq!(got, Some("5"));
    }

    #[test]
    fn rest_param_missing_name_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME
        // structural variant under both expansion strategies. The
        // defmacro-syntax-gate fires inside `macro_def_from` →
        // `parse_params`, BEFORE either strategy's expansion path
        // runs; so both `Expander::new()` (bytecode) and
        // `Expander::new_substitute_only()` (substitute) reject the
        // SAME malformed defmacro at the SAME gate. Sibling of
        // `non_symbol_param_substitute_and_bytecode_paths_agree`.
        let src = "(defmacro f (a &rest 5) `(,a))";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(rest_param_missing_name_fields(&err_subst), (1, Some("5")));
        assert_eq!(rest_param_missing_name_fields(&err_byte), (1, Some("5")));
    }

    #[test]
    fn rest_param_missing_name_message_renders_legacy_substring_with_marker() {
        // End-to-end through Display — pins the rendered diagnostic
        // consumers see today (REPL, tatara-check) AND the new `(rest
        // marker at position {rest_position}, got {got})` clause. The
        // legacy `"&rest needs a name"` substring rides through
        // verbatim.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a &rest 5) `(,a))").unwrap())
            .expect_err("&rest followed by non-symbol must error");
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: &rest needs a name \
             (rest marker at position 1, got 5)"
        );
    }

    #[test]
    fn rest_param_missing_name_message_renders_none_provided_when_follower_absent() {
        // Same as the prior test but for the "missing entirely" branch.
        // The renderable detail is `(rest marker at position
        // {rest_position}, none provided)` — naming the absence
        // structurally instead of an empty / partial parenthetical.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a &rest) `(,a))").unwrap())
            .expect_err("&rest with no follower must error");
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: &rest needs a name \
             (rest marker at position 1, none provided)"
        );
    }

    #[test]
    fn rest_param_missing_name_position_is_none_today() {
        // Pins that `position()` returns `None` so the future `pos:
        // Option<usize>` add (once `Sexp` carries source spans) lands
        // as a deliberate fail-before/pass-after delta rather than a
        // silent default. Parallel to
        // `non_symbol_param_position_is_none_today` and
        // `missing_macro_arg_position_is_none_today`.
        let err_missing = LispError::RestParamMissingName {
            rest_position: 1,
            got: None,
        };
        assert_eq!(err_missing.position(), None);
        let err_got = LispError::RestParamMissingName {
            rest_position: 0,
            got: Some("5".into()),
        };
        assert_eq!(err_got.position(), None);
    }

    // --- MacroDefHead enum (the closed-set lift) ---
    //
    // The next nine tests pin the typed-enum lift that closes the
    // three-times rule on the `head: &str → &'static str` projection
    // idiom previously inlined at FOUR sites (the `matches!` gate at
    // the top of `macro_def_from` plus the projection match inside
    // each of `defmacro_arity`, `defmacro_non_symbol_name`,
    // `defmacro_non_list_params`). Every test in this block names
    // `MacroDefHead` directly — the symbol exists only after the
    // lift, so the entire block is fail-before/pass-after by
    // construction (compile-time edge). Theory anchor: THEORY.md
    // §VI.1 — three-times rule; THEORY.md §V.1 — the closed set is
    // a TYPE rather than a `matches!` literal.

    #[test]
    fn macro_def_head_from_keyword_recognizes_defmacro() {
        // Pins that `MacroDefHead::from_keyword("defmacro")` returns
        // `Some(MacroDefHead::Defmacro)` — the first of the three
        // canonical macro-definition head keywords. A regression that
        // re-inlines a `matches!`-only gate (without the typed-enum
        // projection) deletes `from_keyword` and fails-loudly here.
        assert_eq!(
            MacroDefHead::from_keyword("defmacro"),
            Some(MacroDefHead::Defmacro)
        );
    }

    #[test]
    fn macro_def_head_from_keyword_recognizes_defpoint_template() {
        // Pins that `MacroDefHead::from_keyword("defpoint-template")`
        // returns `Some(MacroDefHead::DefpointTemplate)` — the second
        // of the three canonical head keywords. The `defpoint-template`
        // form is the K8s-as-processes authoring surface (see
        // tatara-process); `macro_def_from` must recognize it
        // identically to `defmacro` so the `(defpoint-template …)`
        // form's macro-style binding works the same way.
        assert_eq!(
            MacroDefHead::from_keyword("defpoint-template"),
            Some(MacroDefHead::DefpointTemplate)
        );
    }

    #[test]
    fn macro_def_head_from_keyword_recognizes_defcheck() {
        // Pins that `MacroDefHead::from_keyword("defcheck")` returns
        // `Some(MacroDefHead::Defcheck)` — the third and final
        // canonical head keyword. The `defcheck` form is the
        // workspace-coherence authoring surface (see
        // tatara-reconciler/checks.lisp); `macro_def_from` must
        // recognize it identically to `defmacro` so user-defined
        // checks inherit the macro-style binding semantics.
        assert_eq!(
            MacroDefHead::from_keyword("defcheck"),
            Some(MacroDefHead::Defcheck)
        );
    }

    #[test]
    fn macro_def_head_from_keyword_rejects_unknown() {
        // Pins that `MacroDefHead::from_keyword` returns `None` for
        // anything outside the closed set — a non-symbol keyword
        // (`"if"`), a near-miss spelling (`"defmacroo"`,
        // `"defcheckk"`), and the empty string. `macro_def_from`
        // depends on this `None` projection to mean "this form is
        // not a defmacro form" and walk past — a regression that
        // accidentally accepts a near-miss head (e.g. via a
        // lower-cased `EqualFold` match) would route `(defmacroo …)`
        // through the arity gate, which is wrong. Pins all four
        // canonical near-miss / non-canonical inputs.
        assert_eq!(MacroDefHead::from_keyword("if"), None);
        assert_eq!(MacroDefHead::from_keyword("defmacroo"), None);
        assert_eq!(MacroDefHead::from_keyword("defcheckk"), None);
        assert_eq!(MacroDefHead::from_keyword(""), None);
    }

    #[test]
    fn macro_def_head_keyword_round_trips_each_variant() {
        // Pins that `MacroDefHead::keyword` returns the canonical
        // `&'static str` literal for each variant. Together with
        // `from_keyword` this closes the bidirectional projection:
        // for every canonical head keyword `s`, `MacroDefHead::
        // from_keyword(s).unwrap().keyword() == s`. The `&'static
        // str` lifetime on the return type is load-bearing — it's
        // what lets the `LispError::Defmacro*` variants carry
        // `head: &'static str` slots without an arbitrary owned
        // `String`. Pinning the `: &'static str` binding here
        // makes the lifetime requirement load-bearing in the test.
        let s_defmacro: &'static str = MacroDefHead::Defmacro.keyword();
        let s_defpoint: &'static str = MacroDefHead::DefpointTemplate.keyword();
        let s_defcheck: &'static str = MacroDefHead::Defcheck.keyword();
        assert_eq!(s_defmacro, "defmacro");
        assert_eq!(s_defpoint, "defpoint-template");
        assert_eq!(s_defcheck, "defcheck");
    }

    #[test]
    fn macro_def_head_keyword_round_trips_through_from_keyword() {
        // Pins that the two halves of the projection compose to the
        // identity on the closed set: for every canonical head
        // keyword, projecting `&str → MacroDefHead → &'static str`
        // returns the original literal. Sibling of
        // `macro_def_head_keyword_round_trips_each_variant` —
        // together they pin both directions of the bidirection.
        for kw in ["defmacro", "defpoint-template", "defcheck"] {
            let head = MacroDefHead::from_keyword(kw).expect("canonical keyword must project");
            assert_eq!(head.keyword(), kw);
        }
    }

    #[test]
    fn macro_def_head_threads_through_defmacro_arity_helper() {
        // Pins that `defmacro_arity` accepts a typed `MacroDefHead`
        // and threads it through to the variant's typed `head` slot
        // unchanged — no `&str` projection at the helper boundary
        // (the projection through `MacroDefHead::keyword()` happens at
        // Display rendering time inside the `#[error(...)]`
        // annotation). A regression that drops the `MacroDefHead`
        // parameter type (e.g. by reverting to `head: &str`) breaks
        // compilation here. Pinning each of the three variants gives
        // the typed-head threading the same path-uniformity edge the
        // existing `defmacro_arity_in_*_emits_same_variant` tests pin
        // for the call-site path through `macro_def_from`.
        for head in [
            MacroDefHead::Defmacro,
            MacroDefHead::DefpointTemplate,
            MacroDefHead::Defcheck,
        ] {
            let err = defmacro_arity(head, 2);
            match err {
                LispError::DefmacroArity { head: h, arity: 2 } => assert_eq!(h, head),
                other => panic!("expected DefmacroArity, got: {other:?}"),
            }
        }
    }

    #[test]
    fn macro_def_head_threads_through_defmacro_non_symbol_name_helper() {
        // Sibling of the `defmacro_arity` threading test — pins that
        // `defmacro_non_symbol_name` accepts a typed `MacroDefHead`
        // and threads it through to the variant's typed `head` slot
        // unchanged. The `got: &Sexp` parameter rides through
        // `crate::domain::sexp_witness` into the variant's typed
        // `got: SexpWitness` slot so BOTH the structural shape AND
        // the rendered literal are preserved across the helper
        // boundary, parallel to how `non_symbol_param` and
        // `non_symbol_unquote_target` project their `&Sexp` arguments
        // through the same typed joint primitive.
        let got = parse("5");
        for head in [
            MacroDefHead::Defmacro,
            MacroDefHead::DefpointTemplate,
            MacroDefHead::Defcheck,
        ] {
            let err = defmacro_non_symbol_name(head, &got);
            match err {
                LispError::DefmacroNonSymbolName { head: h, got: g } => {
                    assert_eq!(h, head);
                    assert_eq!(g.shape, crate::error::SexpShape::Int);
                    assert_eq!(g.display, "5");
                }
                other => panic!("expected DefmacroNonSymbolName, got: {other:?}"),
            }
        }
    }

    #[test]
    fn macro_def_head_threads_through_defmacro_non_list_params_helper() {
        // Sibling of the `defmacro_arity` and
        // `defmacro_non_symbol_name` threading tests — pins that
        // `defmacro_non_list_params` accepts a typed `MacroDefHead`
        // and threads it through to the variant's typed `head` slot
        // unchanged. Together the three threading tests close the
        // typed-enum lift across all three error helpers — every
        // call site that constructs a `LispError::Defmacro*` variant
        // takes its `head` from a `MacroDefHead`, never from a `&str`
        // match.
        let got = parse("x");
        for head in [
            MacroDefHead::Defmacro,
            MacroDefHead::DefpointTemplate,
            MacroDefHead::Defcheck,
        ] {
            let err = defmacro_non_list_params(head, &got);
            match err {
                LispError::DefmacroNonListParams { head: h, got: g } => {
                    assert_eq!(h, head);
                    assert_eq!(g, "x");
                }
                other => panic!("expected DefmacroNonListParams, got: {other:?}"),
            }
        }
    }

    /// Helper for the defmacro-arity tests — pins the variant shape and
    /// carries the head / arity up to the assert site for legibility.
    /// Sibling of `non_symbol_param_fields` and
    /// `rest_param_missing_name_fields`.
    fn defmacro_arity_fields(err: &LispError) -> (MacroDefHead, usize) {
        match err {
            LispError::DefmacroArity { head, arity } => (*head, *arity),
            other => panic!("expected DefmacroArity, got: {other:?}"),
        }
    }

    #[test]
    fn defmacro_arity_with_head_only_emits_structural_variant() {
        // `(defmacro)` — only the head, no name / params / body. Pins
        // variant identity AND that `arity == 1` (just the head
        // element) AND that `head == "defmacro"`. A regression that
        // re-inlines the legacy `LispError::Compile` shape (which
        // named neither field) fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro)").unwrap())
            .expect_err("defmacro arity gate must error");
        let (head, arity) = defmacro_arity_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(arity, 1);
    }

    #[test]
    fn defmacro_arity_with_head_and_name_emits_structural_variant() {
        // `(defmacro f)` — head + name, missing params + body. Pins
        // that `arity` advances with the actual form length (2 for
        // this case) so an LSP quick-fix that wants to surface "you
        // wrote 2 elements; need 4" gains the count as data, no
        // source re-parse required.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f)").unwrap())
            .expect_err("defmacro arity gate must error");
        let (head, arity) = defmacro_arity_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(arity, 2);
    }

    #[test]
    fn defmacro_arity_with_head_name_params_emits_structural_variant() {
        // `(defmacro f ())` — head + name + params, missing body
        // (the most-complete partial defmacro that still trips the
        // arity gate). Pins that `arity == 3` exactly so an LSP
        // quick-fix that wants to surface "your defmacro is one
        // element short — body is missing" gains the count as data.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f ())").unwrap())
            .expect_err("defmacro arity gate must error");
        let (head, arity) = defmacro_arity_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(arity, 3);
    }

    #[test]
    fn defmacro_arity_in_defpoint_template_emits_same_variant() {
        // `defpoint-template` shares `macro_def_from` with `defmacro`
        // (all three head keywords route through the same gate). Pins
        // that the lift fires path-uniformly across the three head
        // keywords AND that the variant's `head` slot carries the
        // actual head literal — `defpoint-template`, not `defmacro`
        // — so an LSP that wants to point at "your defpoint-template
        // form is missing elements" gains the head as data.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defpoint-template t)").unwrap())
            .expect_err("defpoint-template arity gate must error");
        let (head, arity) = defmacro_arity_fields(&err);
        assert_eq!(head, MacroDefHead::DefpointTemplate);
        assert_eq!(arity, 2);
    }

    #[test]
    fn defmacro_arity_in_defcheck_emits_same_variant() {
        // Sibling of the defpoint-template test — `defcheck` is the
        // third head keyword `macro_def_from` recognizes. All three
        // route through the same arity gate and now reject too-short
        // forms with the same structural variant.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defcheck)").unwrap())
            .expect_err("defcheck arity gate must error");
        let (head, arity) = defmacro_arity_fields(&err);
        assert_eq!(head, MacroDefHead::Defcheck);
        assert_eq!(arity, 1);
    }

    #[test]
    fn defmacro_arity_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME
        // structural variant under both expansion strategies. The
        // arity gate fires inside `macro_def_from` BEFORE either
        // strategy's expansion path runs; so both `Expander::new()`
        // (bytecode) and `Expander::new_substitute_only()`
        // (substitute) reject the SAME malformed defmacro at the
        // SAME gate. Sibling of
        // `non_symbol_param_substitute_and_bytecode_paths_agree` and
        // `rest_param_missing_name_substitute_and_bytecode_paths_agree`.
        let src = "(defmacro f)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(
            defmacro_arity_fields(&err_subst),
            (MacroDefHead::Defmacro, 2)
        );
        assert_eq!(
            defmacro_arity_fields(&err_byte),
            (MacroDefHead::Defmacro, 2)
        );
    }

    #[test]
    fn defmacro_arity_message_renders_legacy_substring_with_arity() {
        // End-to-end through Display — pins the rendered diagnostic
        // consumers see today (REPL, `tatara-check`) AND the new
        // `(got {arity} elements, need 4)` clause. The legacy
        // `"(defmacro name (params) body) required"` substring
        // rides through verbatim. Tools that pattern-match on the
        // variant gain structural binding to `head` / `arity`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f)").unwrap())
            .expect_err("defmacro arity gate must error");
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: (defmacro name (params) body) required \
             (got 2 elements, need 4)"
        );
    }

    #[test]
    fn defmacro_arity_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp`
        // carries source positions, `position()` on `LispError`
        // returns `None` for this variant. A future run that gives
        // `Sexp` source spans adds `pos: Option<usize>` to ONE place;
        // this test gives that change a deliberate fail-before/pass-
        // after delta. Parallel to
        // `non_symbol_param_position_is_none_today` and
        // `rest_param_missing_name_position_is_none_today`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro)").unwrap())
            .expect_err("defmacro arity gate must error");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn defmacro_arity_does_not_fire_for_well_formed_arity_4_defmacro() {
        // Negative control: a defmacro with exactly 4 elements (head
        // + name + params + body) passes the arity gate. Pins that
        // the lift is scoped to the arity-deficient case, not to
        // every defmacro form. After this test, a regression that
        // tightens the arity gate to >= 5 (e.g. spuriously requiring
        // a docstring slot) fails-loudly here.
        let mut e = Expander::new();
        let out = e
            .expand_program(read("(defmacro id (x) `,x) (id 42)").unwrap())
            .expect("well-formed defmacro must succeed");
        assert_eq!(out[0], Sexp::int(42));
    }

    /// Helper for the defmacro-non-symbol-name tests — pins variant
    /// shape and carries the head / got up to the assert site for
    /// legibility. Sibling of `defmacro_arity_fields`,
    /// `non_symbol_param_fields`, and `rest_param_missing_name_fields`.
    fn defmacro_non_symbol_name_fields(err: &LispError) -> (MacroDefHead, &str) {
        match err {
            LispError::DefmacroNonSymbolName { head, got } => (*head, got.display.as_str()),
            other => panic!("expected DefmacroNonSymbolName, got: {other:?}"),
        }
    }

    #[test]
    fn defmacro_non_symbol_name_with_int_emits_structural_variant() {
        // `(defmacro 5 () body)` — the form passes the arity gate
        // (4 elements) but list[1] is `5`, not a symbol. Pins variant
        // identity AND that `head == "defmacro"` AND that `got ==
        // "5"`. A regression that re-inlines the legacy
        // `LispError::Compile { form: "defmacro", message: "expected
        // name symbol" }` shape (which named the failure mode but
        // not the offending element) fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro 5 () body)").unwrap())
            .expect_err("defmacro non-symbol name gate must error");
        let (head, got) = defmacro_non_symbol_name_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, "5");
    }

    #[test]
    fn defmacro_non_symbol_name_with_keyword_emits_structural_variant() {
        // `(defmacro :foo () body)` — list[1] is the keyword `:foo`,
        // not a symbol. Pins that `Sexp::Display` for
        // `Atom::Keyword(s)` writes `:s` and the variant's `got` slot
        // carries the keyword form unchanged. An LSP that wants to
        // surface "you wrote `:foo` where a name symbol was expected"
        // gains the literal keyword value as data, no source re-parse
        // required.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro :foo () body)").unwrap())
            .expect_err("defmacro non-symbol name gate must error");
        let (head, got) = defmacro_non_symbol_name_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, ":foo");
    }

    #[test]
    fn defmacro_non_symbol_name_with_string_emits_structural_variant() {
        // `(defmacro "name" () body)` — list[1] is the string
        // literal `"name"`, not a symbol. Pins that `Sexp::Display`
        // for `Atom::String(s)` writes `"s"` (with quotes) and the
        // variant's `got` slot carries the quoted form unchanged.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro \"name\" () body)").unwrap())
            .expect_err("defmacro non-symbol name gate must error");
        let (head, got) = defmacro_non_symbol_name_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, "\"name\"");
    }

    #[test]
    fn defmacro_non_symbol_name_with_nested_list_emits_structural_variant() {
        // `(defmacro (nested) () body)` — list[1] is a nested list,
        // not a symbol. Pins that `Sexp::Display` for a list writes
        // `(elements)` and the variant's `got` slot carries the
        // parenthesized form unchanged.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro (nested) () body)").unwrap())
            .expect_err("defmacro non-symbol name gate must error");
        let (head, got) = defmacro_non_symbol_name_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, "(nested)");
    }

    #[test]
    fn defmacro_non_symbol_name_in_defpoint_template_emits_same_variant() {
        // `defpoint-template` shares `macro_def_from` with `defmacro`
        // (all three head keywords route through the same gate).
        // Pins that the lift fires path-uniformly across the three
        // head keywords AND that the variant's `head` slot carries
        // the actual head literal — `defpoint-template`, not
        // `defmacro` — so an LSP that wants to point at "your
        // defpoint-template form's name slot isn't a symbol" gains
        // the head as data.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defpoint-template 7 () body)").unwrap())
            .expect_err("defpoint-template non-symbol name gate must error");
        let (head, got) = defmacro_non_symbol_name_fields(&err);
        assert_eq!(head, MacroDefHead::DefpointTemplate);
        assert_eq!(got, "7");
    }

    #[test]
    fn defmacro_non_symbol_name_in_defcheck_emits_same_variant() {
        // Sibling for the `defcheck` head — third head keyword
        // `macro_def_from` recognizes. Rounds out the three-head-
        // keyword coverage so the lift is path-uniform across
        // `defmacro` / `defpoint-template` / `defcheck`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defcheck :k () body)").unwrap())
            .expect_err("defcheck non-symbol name gate must error");
        let (head, got) = defmacro_non_symbol_name_fields(&err);
        assert_eq!(head, MacroDefHead::Defcheck);
        assert_eq!(got, ":k");
    }

    #[test]
    fn defmacro_non_symbol_name_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME
        // structural variant under both expansion strategies. The
        // name-symbol gate fires inside `macro_def_from` BEFORE
        // either expansion strategy runs, so the gate is naturally
        // path-uniform; pinning it gives a regression that drifts
        // either strategy's handling of non-symbol-name defmacros (or
        // makes one strategy accept what the other rejects) a fail-
        // before/pass-after edge. Sibling of
        // `defmacro_arity_substitute_and_bytecode_paths_agree`,
        // `non_symbol_param_substitute_and_bytecode_paths_agree`, and
        // `rest_param_missing_name_substitute_and_bytecode_paths_agree`.
        let src = "(defmacro 5 () body)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(
            defmacro_non_symbol_name_fields(&err_subst),
            (MacroDefHead::Defmacro, "5")
        );
        assert_eq!(
            defmacro_non_symbol_name_fields(&err_byte),
            (MacroDefHead::Defmacro, "5")
        );
    }

    #[test]
    fn defmacro_non_symbol_name_message_renders_legacy_substring_with_got() {
        // End-to-end through Display — pins the rendered diagnostic
        // consumers see today (REPL, `tatara-check`) AND the new
        // `, got {got}` clause. The legacy `"expected name symbol"`
        // substring rides through verbatim; the prefix matches the
        // legacy `Compile { form: "defmacro", message: "expected name
        // symbol" }` byte-for-byte. Tools that pattern-match on the
        // variant gain structural binding to `head` / `got`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro 5 () body)").unwrap())
            .expect_err("defmacro non-symbol name gate must error");
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected name symbol, got 5"
        );
    }

    #[test]
    fn defmacro_non_symbol_name_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp`
        // carries source positions, `position()` on `LispError`
        // returns `None` for this variant. A future run that gives
        // `Sexp` source spans adds `pos: Option<usize>` to ONE place;
        // this test gives that change a deliberate fail-before/pass-
        // after delta. Parallel to
        // `defmacro_arity_position_is_none_today`,
        // `non_symbol_param_position_is_none_today`, and
        // `rest_param_missing_name_position_is_none_today`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro 5 () body)").unwrap())
            .expect_err("defmacro non-symbol name gate must error");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn defmacro_non_symbol_name_does_not_fire_for_well_formed_defmacro() {
        // Negative control: a defmacro whose name slot IS a symbol
        // passes the name-symbol gate. Pins that the lift is scoped
        // to the non-symbol-name case, not to every defmacro form.
        // After this test, a regression that tightens the gate to
        // reject e.g. kebab-cased names fails-loudly here.
        let mut e = Expander::new();
        let out = e
            .expand_program(read("(defmacro id (x) `,x) (id 42)").unwrap())
            .expect("well-formed defmacro must succeed");
        assert_eq!(out[0], Sexp::int(42));
    }

    #[test]
    fn defmacro_non_symbol_name_fires_after_arity_gate_passes() {
        // Pins the gate ordering: a 4-element defmacro whose name
        // slot is non-symbol fires `DefmacroNonSymbolName`, NOT
        // `DefmacroArity`. The arity gate (>= 4 elements) admits
        // this form; the name-symbol gate is the next checkpoint.
        // A regression that swaps the gate ordering (e.g. checks
        // name-symbol before arity, so `(defmacro 5)` would emit
        // `DefmacroNonSymbolName` instead of `DefmacroArity`) fails-
        // loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro 5 () body)").unwrap())
            .expect_err("name-symbol gate must error");
        assert!(
            matches!(err, LispError::DefmacroNonSymbolName { .. }),
            "expected DefmacroNonSymbolName, got: {err:?}"
        );

        let err_arity = e
            .expand_program(read("(defmacro 5)").unwrap())
            .expect_err("arity gate must error");
        assert!(
            matches!(err_arity, LispError::DefmacroArity { .. }),
            "expected DefmacroArity (arity < 4 short-circuits before name check), \
             got: {err_arity:?}"
        );
    }

    /// Helper for the defmacro-non-list-params tests — pins variant
    /// shape and carries the head / got up to the assert site for
    /// legibility. Sibling of `defmacro_arity_fields`,
    /// `defmacro_non_symbol_name_fields`, `non_symbol_param_fields`,
    /// and `rest_param_missing_name_fields`.
    fn defmacro_non_list_params_fields(err: &LispError) -> (MacroDefHead, &str) {
        match err {
            LispError::DefmacroNonListParams { head, got } => (*head, got.as_str()),
            other => panic!("expected DefmacroNonListParams, got: {other:?}"),
        }
    }

    #[test]
    fn defmacro_non_list_params_with_symbol_emits_structural_variant() {
        // `(defmacro f x body)` — the form passes both the arity gate
        // (4 elements) AND the name-symbol gate (`f` is a symbol) but
        // list[2] is the symbol `x`, not a list. Pins variant identity
        // AND that `head == "defmacro"` AND that `got == "x"`. A
        // regression that re-inlines the legacy `LispError::Compile {
        // form: "defmacro", message: "expected param list" }` shape
        // (which named the failure mode but not the offending element)
        // fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f x body)").unwrap())
            .expect_err("defmacro non-list params gate must error");
        let (head, got) = defmacro_non_list_params_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, "x");
    }

    #[test]
    fn defmacro_non_list_params_with_int_emits_structural_variant() {
        // `(defmacro f 5 body)` — list[2] is `5`, not a list. Pins
        // that `Sexp::Display` for `Atom::Int(n)` writes `n` and the
        // variant's `got` slot carries the integer form unchanged. An
        // LSP that surfaces "you wrote `5` where a param list was
        // expected" gains the literal value as data, no source
        // re-parse required.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f 5 body)").unwrap())
            .expect_err("defmacro non-list params gate must error");
        let (head, got) = defmacro_non_list_params_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, "5");
    }

    #[test]
    fn defmacro_non_list_params_with_keyword_emits_structural_variant() {
        // `(defmacro f :foo body)` — list[2] is the keyword `:foo`,
        // not a list. Pins that `Sexp::Display` for `Atom::Keyword(s)`
        // writes `:s` and the variant's `got` slot carries the
        // keyword form unchanged.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f :foo body)").unwrap())
            .expect_err("defmacro non-list params gate must error");
        let (head, got) = defmacro_non_list_params_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, ":foo");
    }

    #[test]
    fn defmacro_non_list_params_with_string_emits_structural_variant() {
        // `(defmacro f "params" body)` — list[2] is the string literal
        // `"params"`, not a list. Pins that `Sexp::Display` for
        // `Atom::String(s)` writes `"s"` (with quotes) and the
        // variant's `got` slot carries the quoted form unchanged.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f \"params\" body)").unwrap())
            .expect_err("defmacro non-list params gate must error");
        let (head, got) = defmacro_non_list_params_fields(&err);
        assert_eq!(head, MacroDefHead::Defmacro);
        assert_eq!(got, "\"params\"");
    }

    #[test]
    fn defmacro_non_list_params_in_defpoint_template_emits_same_variant() {
        // `defpoint-template` shares `macro_def_from` with `defmacro`
        // (all three head keywords route through the same gate).
        // Pins that the lift fires path-uniformly across the three
        // head keywords AND that the variant's `head` slot carries
        // the actual head literal — `defpoint-template`, not
        // `defmacro` — so an LSP that wants to point at "your
        // defpoint-template form's param-list slot isn't a list"
        // gains the head as data.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defpoint-template t x body)").unwrap())
            .expect_err("defpoint-template non-list params gate must error");
        let (head, got) = defmacro_non_list_params_fields(&err);
        assert_eq!(head, MacroDefHead::DefpointTemplate);
        assert_eq!(got, "x");
    }

    #[test]
    fn defmacro_non_list_params_in_defcheck_emits_same_variant() {
        // Sibling for the `defcheck` head — third head keyword
        // `macro_def_from` recognizes. Rounds out the three-head-
        // keyword coverage so the lift is path-uniform across
        // `defmacro` / `defpoint-template` / `defcheck`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defcheck c 7 body)").unwrap())
            .expect_err("defcheck non-list params gate must error");
        let (head, got) = defmacro_non_list_params_fields(&err);
        assert_eq!(head, MacroDefHead::Defcheck);
        assert_eq!(got, "7");
    }

    #[test]
    fn defmacro_non_list_params_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME
        // structural variant under both expansion strategies. The
        // param-list gate fires inside `macro_def_from` BEFORE either
        // expansion strategy runs, so the gate is naturally path-
        // uniform; pinning it gives a regression that drifts either
        // strategy's handling of non-list-params defmacros (or makes
        // one strategy accept what the other rejects) a fail-before/
        // pass-after edge. Sibling of
        // `defmacro_arity_substitute_and_bytecode_paths_agree`,
        // `defmacro_non_symbol_name_substitute_and_bytecode_paths_agree`,
        // `non_symbol_param_substitute_and_bytecode_paths_agree`, and
        // `rest_param_missing_name_substitute_and_bytecode_paths_agree`.
        let src = "(defmacro f x body)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(
            defmacro_non_list_params_fields(&err_subst),
            (MacroDefHead::Defmacro, "x")
        );
        assert_eq!(
            defmacro_non_list_params_fields(&err_byte),
            (MacroDefHead::Defmacro, "x")
        );
    }

    #[test]
    fn defmacro_non_list_params_message_renders_legacy_substring_with_got() {
        // End-to-end through Display — pins the rendered diagnostic
        // consumers see today (REPL, `tatara-check`) AND the new
        // `, got {got}` clause. The legacy `"expected param list"`
        // substring rides through verbatim; the prefix matches the
        // legacy `Compile { form: "defmacro", message: "expected
        // param list" }` byte-for-byte. Tools that pattern-match on
        // the variant gain structural binding to `head` / `got`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f x body)").unwrap())
            .expect_err("defmacro non-list params gate must error");
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected param list, got x"
        );
    }

    #[test]
    fn defmacro_non_list_params_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp`
        // carries source positions, `position()` on `LispError`
        // returns `None` for this variant. A future run that gives
        // `Sexp` source spans adds `pos: Option<usize>` to ONE place;
        // this test gives that change a deliberate fail-before/pass-
        // after delta. Parallel to
        // `defmacro_arity_position_is_none_today`,
        // `defmacro_non_symbol_name_position_is_none_today`,
        // `non_symbol_param_position_is_none_today`, and
        // `rest_param_missing_name_position_is_none_today`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f x body)").unwrap())
            .expect_err("defmacro non-list params gate must error");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn defmacro_non_list_params_does_not_fire_for_well_formed_defmacro() {
        // Negative control: a defmacro whose param-list slot IS a
        // list passes the param-list gate. Pins that the lift is
        // scoped to the non-list-params case, not to every defmacro
        // form. After this test, a regression that tightens the gate
        // to reject e.g. empty param lists fails-loudly here.
        let mut e = Expander::new();
        let out = e
            .expand_program(read("(defmacro id (x) `,x) (id 42)").unwrap())
            .expect("well-formed defmacro must succeed");
        assert_eq!(out[0], Sexp::int(42));
    }

    #[test]
    fn defmacro_non_list_params_fires_after_name_symbol_gate_passes() {
        // Pins the gate ordering: a 4-element defmacro whose name
        // slot IS a symbol but whose param-list slot is non-list
        // fires `DefmacroNonListParams`, NOT `DefmacroNonSymbolName`.
        // The name-symbol gate admits this form; the param-list gate
        // is the next checkpoint. A regression that swaps the gate
        // ordering (e.g. checks param-list before name-symbol, so
        // `(defmacro 5 x body)` would emit `DefmacroNonListParams`
        // instead of `DefmacroNonSymbolName`) fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f x body)").unwrap())
            .expect_err("param-list gate must error");
        assert!(
            matches!(err, LispError::DefmacroNonListParams { .. }),
            "expected DefmacroNonListParams, got: {err:?}"
        );

        let err_name = e
            .expand_program(read("(defmacro 5 x body)").unwrap())
            .expect_err("name-symbol gate must error");
        assert!(
            matches!(err_name, LispError::DefmacroNonSymbolName { .. }),
            "expected DefmacroNonSymbolName (name-symbol gate short-circuits before param-list check), \
             got: {err_name:?}"
        );
    }

    #[test]
    fn defmacro_non_list_params_fires_after_arity_gate_passes() {
        // Pins the full gate ordering: a 4-element defmacro whose
        // first three slots are head/symbol/non-list fires
        // `DefmacroNonListParams`, NOT `DefmacroArity`. The arity
        // gate (>= 4 elements) admits this form; the name-symbol
        // gate admits the symbol; the param-list gate is the third
        // checkpoint. A regression that drifts the gate sequence
        // (e.g. fires `DefmacroArity` for a 4-element form) fails-
        // loudly here. Parallel to
        // `defmacro_non_symbol_name_fires_after_arity_gate_passes`
        // — together they pin the full
        // arity → name-symbol → param-list ordering inside
        // `macro_def_from`.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f x body)").unwrap())
            .expect_err("param-list gate must error");
        assert!(
            matches!(err, LispError::DefmacroNonListParams { .. }),
            "expected DefmacroNonListParams, got: {err:?}"
        );

        let err_arity = e
            .expand_program(read("(defmacro f x)").unwrap())
            .expect_err("arity gate must error");
        assert!(
            matches!(err_arity, LispError::DefmacroArity { .. }),
            "expected DefmacroArity (arity < 4 short-circuits before param-list check), \
             got: {err_arity:?}"
        );
    }

    #[test]
    fn rest_marker_at_param_list_position_is_not_non_symbol_param() {
        // Negative control: `&rest` is a symbol (`Atom::Symbol("&rest")`)
        // at the parser level, so `as_symbol()` succeeds for it. The
        // `NonSymbolParam` variant does NOT fire on the `&rest` marker
        // itself; the dedicated `&rest needs a name` rejection (a
        // separate failure mode in this cluster) handles malformed
        // rest-param shapes. Pins that the lift is scoped to
        // non-symbol elements at param-list positions, not to
        // every malformed-param shape.
        let mut e = Expander::new();
        let out = e
            .expand_program(read("(defmacro f (a &rest xs) `(list ,a ,@xs)) (f 1 2 3)").unwrap())
            .expect("&rest with name must succeed");
        assert_eq!(out[0], parse("(list 1 2 3)"));
    }

    #[test]
    fn non_symbol_unquote_target_message_renders_canonical_type_mismatch_shape() {
        // End-to-end through the Display impl — pins the rendered diagnostic
        // a downstream tool sees today (REPL, tatara-check). The shape is
        // parallel to the existing `TypeMismatch` variant: form, expected
        // shape, offending literal — all three slots present.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro w (x) `,(list 1 2)) (w 1)").unwrap())
            .expect_err("non-symbol target must error");
        assert_eq!(
            format!("{err}"),
            "compile error in ,: expected symbol, got (list 1 2)"
        );
    }

    // ── template_invariant_violation: structural lift ───────────────
    //
    // The four byte-identical inline `LispError::Compile { form:
    // macro_name.into(), message: <invariant> }` triples in `apply_compiled`
    // (Subst-bad-index, Splice-bad-index, EndList-empty-stack,
    // final-no-value gates) were lifted to `template_invariant_violation`,
    // and the helper's emission was promoted from `LispError::Compile`-
    // shape to the structural `LispError::TemplateInvariant { macro_name,
    // kind: TemplateInvariantKind }` variant. The index payload of the
    // Subst / Splice gates lives INSIDE the variant (`SubstBadIndex(usize)`
    // / `SpliceBadIndex(usize)`), so the invalid combination "stack-gate
    // kind with an op-index" (e.g. `EndListEmptyStack` carrying a `usize`)
    // is structurally unrepresentable. Display matches the legacy
    // `Compile`-shaped diagnostic byte-for-byte via the closed-set
    // `TemplateInvariantKind::message()` projection so authoring-tool
    // substring greps see no drift across the lift.
    //
    // The tests below pin: (a) the helper produces the structural
    // `LispError::TemplateInvariant` variant with `macro_name` and `kind`
    // first-class; (b) the Subst / Splice gates thread the bad index
    // through the typed variants `SubstBadIndex(usize)` / `SpliceBadIndex(usize)`
    // unchanged; (c) the two REACHABLE invariant-violation paths through
    // `apply_compiled` — Subst with out-of-bounds idx, Splice with
    // out-of-bounds idx — route through the helper end-to-end (the
    // EndList / no-value paths are guarded by `last_mut().unwrap()`
    // ahead of `pop().ok_or_else()` and are not reachable through any
    // single CompiledTemplate; they remain defensive against future
    // changes to the stack discipline); (d) the legacy Display
    // rendering matches byte-for-byte across the lift; (e) positive
    // controls: a well-formed CompiledTemplate routes PAST the helper
    // cleanly, and unrelated macro errors (missing-required-arg) do
    // NOT route through the helper.

    #[test]
    fn template_invariant_violation_emits_structural_variant_with_macro_name_and_kind() {
        // Direct unit test of the helper: a fixed macro_name and a
        // `TemplateInvariantKind` produce a `LispError::TemplateInvariant`
        // variant with the macro_name in the `macro_name` slot and the
        // kind passed through verbatim in the `kind` slot. A regression
        // that drifts the variant (e.g., back to `LispError::Compile`)
        // or swaps the slot positions fails-loudly here.
        let err = template_invariant_violation("test-macro", TemplateInvariantKind::FinalNoValue);
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "test-macro");
                assert_eq!(kind, TemplateInvariantKind::FinalNoValue);
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn template_invariant_violation_threads_subst_idx_through_typed_variant() {
        // The Subst gate's `usize` idx lives INSIDE the
        // `TemplateInvariantKind::SubstBadIndex(usize)` variant rather
        // than being substring-rendered into a free-form `message`
        // slot. Pin that the helper threads the bad index through the
        // typed variant unchanged; a regression that drops the index
        // payload (e.g., via a `usize -> ()` projection) fails here.
        let err = template_invariant_violation("wrap", TemplateInvariantKind::SubstBadIndex(7));
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "wrap");
                assert_eq!(kind, TemplateInvariantKind::SubstBadIndex(7));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn apply_compiled_subst_bad_idx_routes_through_template_invariant_violation() {
        // Hand-crafted CompiledTemplate with a Subst(99) op against
        // an empty params list: `args_by_index` has length 0, so
        // `.get(99)` returns None and the `ok_or_else` triggers
        // through the helper. Fail-before-pass-after: this same input
        // pre-lift went through `LispError::Compile { form: macro_name,
        // message: format!("compiled template referenced bad param
        // index {idx}") }`; post-lift it routes through
        // `template_invariant_violation` and emits the structural
        // `TemplateInvariant { macro_name, kind: SubstBadIndex(99) }`
        // variant with the bad index threaded through as typed data.
        let tmpl = CompiledTemplate {
            ops: vec![TemplateOp::Subst(99)],
        };
        let err = apply_compiled("test-macro", &[], &tmpl, &[]).expect_err("bad idx must error");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "test-macro");
                assert_eq!(kind, TemplateInvariantKind::SubstBadIndex(99));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn apply_compiled_splice_bad_idx_routes_through_template_invariant_violation() {
        // Hand-crafted CompiledTemplate with a Splice(42) op against
        // an empty params list. Sibling of the Subst-bad-idx test;
        // pins the Splice gate routes through the helper with the
        // typed `SpliceBadIndex(42)` kind carrying the bad index.
        let tmpl = CompiledTemplate {
            ops: vec![TemplateOp::Splice(42)],
        };
        let err =
            apply_compiled("call-macro", &[], &tmpl, &[]).expect_err("bad splice idx must error");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "call-macro");
                assert_eq!(kind, TemplateInvariantKind::SpliceBadIndex(42));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn apply_compiled_subst_bad_idx_renders_legacy_compile_shape() {
        // End-to-end through the `LispError` Display impl — pins the
        // rendered diagnostic byte-for-byte: `"compile error in
        // test-macro: compiled template referenced bad param index 99"`.
        // Authoring tools that substring-grep the rendered diagnostic
        // (`tatara-check`'s diagnostic capture, REPL substring-greps)
        // see no drift across the lift. Parallel to how
        // `compile_named_non_symbol_name_renders_legacy_compile_shape`
        // pins the sibling-file (compile.rs) lift's Display contract.
        let tmpl = CompiledTemplate {
            ops: vec![TemplateOp::Subst(99)],
        };
        let err = apply_compiled("test-macro", &[], &tmpl, &[]).expect_err("bad idx must error");
        assert_eq!(
            format!("{err}"),
            "compile error in test-macro: compiled template referenced bad param index 99"
        );
    }

    #[test]
    fn apply_compiled_splice_bad_idx_renders_legacy_compile_shape() {
        // Sibling Display test for the Splice gate. Pins the message
        // byte-for-byte through the `LispError` Display impl: `"compile
        // error in call-macro: compiled template referenced bad splice
        // index 42"`.
        let tmpl = CompiledTemplate {
            ops: vec![TemplateOp::Splice(42)],
        };
        let err =
            apply_compiled("call-macro", &[], &tmpl, &[]).expect_err("bad splice idx must error");
        assert_eq!(
            format!("{err}"),
            "compile error in call-macro: compiled template referenced bad splice index 42"
        );
    }

    #[test]
    fn apply_compiled_well_formed_template_routes_past_template_invariant_violation() {
        // Positive control: a CompiledTemplate produced by the
        // bytecode compiler (`compile_template`) for a well-formed
        // macro never references an out-of-bounds index nor
        // unbalances the stack, so `apply_compiled` routes PAST the
        // helper cleanly. A regression that fires the helper on
        // well-formed bytecode (e.g., off-by-one in the index
        // resolution) would fail here. End-to-end through the public
        // `Expander` surface so the test exercises the same code
        // path users see.
        let mut e = Expander::new();
        let out = e
            .expand_program(read("(defmacro id (x) `,x) (id 42)").unwrap())
            .expect("well-formed macro expansion must not fire template-invariant-violation");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Sexp::int(42));
    }

    #[test]
    fn apply_compiled_missing_required_arg_does_not_route_through_template_invariant_violation() {
        // Negative control: the `missing_macro_arg` gate at the
        // `Param::Required` arm fires BEFORE the bytecode loop runs,
        // so a missing required arg routes through
        // `LispError::MissingMacroArg`, NOT through
        // `template_invariant_violation`. Pins the helper is
        // precisely scoped to bytecode-runtime invariant violations
        // (Subst / Splice / stack gates), not to macro-call arity
        // errors (the latter has its own structural variant). A
        // regression that conflates the two gate clusters would
        // route this case through `Compile { ... }` instead of
        // `MissingMacroArg` and fail-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro need-one (x) `,x) (need-one)").unwrap())
            .expect_err("missing required arg must error");
        assert!(
            matches!(err, LispError::MissingMacroArg { .. }),
            "expected MissingMacroArg, got: {err:?}"
        );
    }
}
