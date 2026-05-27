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
    pub params: MacroParams,
    /// The template body (usually a Quasiquote).
    pub body: Sexp,
}

/// A macro's parameter list — structurally "zero or more required
/// positional params, then an OPTIONAL single `&rest` param."
///
/// This shape promotes the two invariants the reader ([`parse_params`])
/// previously upheld only by construction — `&rest` is LAST, and there is
/// AT MOST ONE of it — from *unobserved discipline* to *unrepresentable
/// state*. The prior representation `Vec<Param>` admitted `[Rest, Required]`
/// (a `&rest` in the middle) and `[Rest, Rest]` (two of them); both are
/// nonsense the binder cannot honor, yet the type permitted them. The flat
/// param INDEX that the bytecode references (`Subst(idx)` / `Splice(idx)`)
/// and the positional binder both walk would silently misalign on such a
/// `Vec` — a `Rest` at index 0 of `[Rest, Required]` makes the binder grab
/// every arg, then fail to bind the trailing `Required`, mapping the
/// template's index-1 substitution onto the wrong value. `MacroParams`
/// cannot express either shape: `rest` is exactly one `Option<String>`,
/// always conceptually after every `required` name.
///
/// The flat-index contract the template bytecode depends on is preserved by
/// [`MacroParams::names`]: index `0..required.len()` are the required names
/// in order, and index `required.len()` (if present) is the rest name —
/// exactly the order the old `Vec<Param>` iterated, since `&rest` was always
/// last. [`MacroParams::bind`] produces the per-index bound values in that
/// same order, so the name-keyed (`bind_args` → `substitute`) and
/// index-keyed (`apply_compiled`) expansion strategies share ONE binder and
/// can never drift.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid states
/// unrepresentable"; the rest-is-last + at-most-one invariants become
/// structural. THEORY.md §VI.1 — generation over composition; the positional
/// binding loop (verbatim in both `bind_args` and `apply_compiled`, the ≥2
/// PRIME-DIRECTIVE trigger) is lifted to ONE owner, `bind`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MacroParams {
    pub required: Vec<String>,
    pub rest: Option<String>,
}

impl MacroParams {
    /// The flat, ordered param-name list the template bytecode indexes into:
    /// every `required` name in order, then the `rest` name if present.
    /// `names()[i]` is the param `Subst(i)` / `Splice(i)` reference.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.required
            .iter()
            .map(String::as_str)
            .chain(self.rest.as_deref())
            .collect()
    }

    /// Bind call args to params positionally, returning the per-index bound
    /// values parallel to [`names`](Self::names): each required name takes
    /// the arg at its position (a missing one is
    /// [`missing_macro_arg`](self::missing_macro_arg)), and a present `rest`
    /// collects every remaining arg into a `Sexp::List` (the empty list when
    /// none remain). Args beyond a rest-less param list are ignored, matching
    /// the prior binder. This is the single binding loop both expansion
    /// strategies share — `apply_compiled` consumes the index vec directly,
    /// `bind_args` zips it against `names()` into the name-keyed map.
    fn bind(&self, macro_name: &str, args: &[Sexp]) -> Result<Vec<Sexp>> {
        let mut out = Vec::with_capacity(self.required.len() + usize::from(self.rest.is_some()));
        for (i, name) in self.required.iter().enumerate() {
            let arg = args
                .get(i)
                .cloned()
                .ok_or_else(|| missing_macro_arg(macro_name, name))?;
            out.push(arg);
        }
        if self.rest.is_some() {
            let rest = args.get(self.required.len()..).unwrap_or(&[]).to_vec();
            out.push(Sexp::List(rest));
        }
        Ok(out)
    }
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
        if let Some((head, args)) = form.as_call() {
            if let Some(def) = self.macros.get(head) {
                let expanded = self.apply(def, args)?;
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
    let names = def.params.names();
    let mut ops = Vec::new();
    compile_node(body, &names, &mut ops)?;
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
            let idx = resolve_unquote_in_params(inner, params, UnquoteForm::Unquote)?;
            ops.push(TemplateOp::Subst(idx));
        }
        Sexp::UnquoteSplice(inner) => {
            let idx = resolve_unquote_in_params(inner, params, UnquoteForm::Splice)?;
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

/// Splice a resolved template value into an in-progress list builder —
/// the SHARED coercion both expansion strategies apply once `,@name`'s
/// gate-1 (must-be-a-symbol) and gate-2 (must-be-bound-in-scope) have
/// resolved the bound value. ONE named primitive the bytecode path
/// (`apply_compiled`'s `TemplateOp::Splice` arm) AND the substitute path
/// (`substitute`'s list-inner `Sexp::UnquoteSplice` arm) share. Before
/// this lift the three-arm coercion —
///
/// ```ignore
/// match value {
///     Sexp::List(items) => builder.extend(items.iter().cloned()),
///     Sexp::Nil         => {}
///     other             => builder.push(other.clone()),
/// }
/// ```
///
/// — was inlined at BOTH sites; the splice RESULT semantics (the last
/// inline-duplicated piece of the splice path after the prior runs lifted
/// gate-1, gate-2, and their composition) lived in two places that MUST
/// agree. After this lift the coercion lives in ONE function, so a
/// regression that drifts one strategy's splice posture from the other —
/// e.g. changing the `Sexp::Nil` arm to push an empty list at the
/// bytecode path but not the substitute path, or coercing a non-list
/// scalar differently across the two strategies — becomes structurally
/// impossible: there is exactly one implementation both strategies call.
///
/// The coercion's three arms ARE the no-evaluator template language's
/// splice contract: a bound LIST flattens its elements into the builder
/// (the canonical splice), a bound NIL contributes nothing (splicing the
/// empty list), and any other bound value splices as a single element (a
/// scalar `,@x` degrades to `,x` rather than erroring — invariant 2's
/// "free middle" lets the macro author rely on this without a
/// mid-rewrite type check; the typed-exit gate re-validates the
/// assembled form). Naming the contract once gives a future gate-3
/// (typed-shape enforcement on bound splice targets) ONE site to wrap
/// rather than two inline arms to keep in lockstep.
///
/// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the two
/// expansion strategies MUST produce identical output for the same
/// (macro, args) pair, and naming the splice coercion once makes that
/// per-strategy agreement structural rather than a two-site discipline
/// the `expansion_layers_agree_on_output_and_cache_wins` benchmark only
/// observes after the fact. THEORY.md §V.1 — knowable platform; the
/// splice RESULT semantics becomes a NAMED primitive authoring tools and
/// future runs bind to. THEORY.md §VI.1 — generation over composition;
/// the two-site coercion is lifted to ONE function, closing the last
/// inline-duplicated piece of the splice path the prior runs' gate lifts
/// (02173dc gate-1, 68da647 gate-2, b456f1f composition) left behind.
fn splice_value_into(builder: &mut Vec<Sexp>, value: &Sexp) {
    match value {
        Sexp::List(items) => builder.extend(items.iter().cloned()),
        Sexp::Nil => {}
        other => builder.push(other.clone()),
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
    params: &MacroParams,
    tmpl: &CompiledTemplate,
    args: &[Sexp],
) -> Result<Sexp> {
    // Resolve args by param index through the shared positional binder —
    // identical semantics to the `bind_args` (substitute) path by construction.
    let args_by_index = params.bind(macro_name, args)?;

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
                splice_value_into(stack.last_mut().unwrap(), v);
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
    let Some(head_str) = form.head_symbol() else {
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

fn parse_params(list: &[Sexp]) -> Result<MacroParams> {
    let mut required = Vec::new();
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
            return Ok(MacroParams {
                required,
                rest: Some(name.to_string()),
            });
        }
        required.push(s.to_string());
        i += 1;
    }
    Ok(MacroParams {
        required,
        rest: None,
    })
}

fn bind_args(
    macro_name: &str,
    params: &MacroParams,
    args: &[Sexp],
) -> Result<HashMap<String, Sexp>> {
    // Zip the shared positional binding (parallel to `names()`) into the
    // name-keyed map the `substitute` path looks substitutions up in.
    let vals = params.bind(macro_name, args)?;
    Ok(params
        .names()
        .into_iter()
        .map(String::from)
        .zip(vals)
        .collect())
}

/// Substitute `,name` and `,@name` within a template.
/// `,@name` only makes sense inside a List — it splices the bound list into
/// the containing list.
fn substitute(form: &Sexp, bindings: &HashMap<String, Sexp>) -> Result<Sexp> {
    match form {
        Sexp::Unquote(inner) => {
            resolve_unquote_in_bindings(inner, bindings, UnquoteForm::Unquote).cloned()
        }
        Sexp::UnquoteSplice(inner) => Err(splice_outside_list(inner)),
        Sexp::List(items) => {
            let mut out: Vec<Sexp> = Vec::with_capacity(items.len());
            for item in items {
                if let Sexp::UnquoteSplice(inner) = item {
                    let val = resolve_unquote_in_bindings(inner, bindings, UnquoteForm::Splice)?;
                    splice_value_into(&mut out, val);
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

/// Project the inner of a `,X` / `,@X` form to its bound symbol name, or
/// raise the structural `LispError::NonSymbolUnquoteTarget` rejection at
/// the typed-entry template-gate boundary. ONE named primitive every
/// `,X` / `,@X` resolution site in the substrate shares — the inline
/// `inner.as_symbol().ok_or_else(|| non_symbol_unquote_target(form,
/// inner))?` pattern appeared four times across `compile_node`
/// (bytecode-path Unquote / UnquoteSplice arms) AND `substitute`
/// (substitute-path Unquote / list-inner UnquoteSplice arms), well past
/// the three-times-rule trigger. After this lift the four sites collapse
/// to a single `unquote_target_symbol(inner, form)?` call, and the
/// substrate's understanding of "an unquote target's first gate is `must
/// be a symbol`" lives in ONE function — a regression that drifts the
/// gate's posture (e.g. accepts non-symbol targets at the bytecode path
/// but not the substitute path) becomes a type-level change at this
/// helper, not a silent four-site divergence.
///
/// Sibling of `non_symbol_unquote_target` (the error builder this gate
/// calls on failure) and `unbound_template_var` (the typed-entry
/// template-gate's SECOND gate — fires once `unquote_target_symbol`
/// projects the symbol successfully but the symbol isn't bound in the
/// in-scope name set). Together the three close the substrate's
/// understanding of the two-step typed-entry template-gate: gate-1 is
/// `must-be-a-symbol`, gate-2 is `must-be-bound-in-scope`. With this
/// lift, gate-1 lives at ONE call boundary across all four template-
/// gate sites — bytecode path AND substitute path AND both `,X` and
/// `,@X` forms.
///
/// `form` is `UnquoteForm` — the closed-set typed enum whose two
/// variants are EXACTLY the two reachable syntactic markers
/// (`Unquote` ⊎ `Splice`). Threading the typed marker through the
/// helper boundary (rather than `&'static str`) lands the same
/// compile-time closed-set guarantee `non_symbol_unquote_target` and
/// `unbound_template_var` get from their `UnquoteForm` slots — a
/// regression that drifts the marker (e.g. a third pseudo-marker call
/// site) becomes a type error at the call site, not a runtime
/// substring drift. The returned `&'a str` borrows from `inner` — the
/// caller feeds it directly into `params.iter().position(|p| *p ==
/// name)` (`compile_node`) or `bindings.get(name)` (`substitute`)
/// without an intermediate allocation.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four
/// inline copies of the gate-1 projection (`compile_node`
/// Unquote/UnquoteSplice + `substitute` Unquote + `substitute`
/// list-inner UnquoteSplice) is past the three-times rule. THEORY.md
/// §V.1 — knowable platform; the gate's identity becomes a NAMED
/// primitive consumer-binding rather than a four-times-inlined
/// match-and-reject snippet — authoring surfaces (REPL, LSP,
/// `tatara-check`) that want to surface "the typed-entry template-gate
/// rejected your form because the unquote target wasn't a symbol" bind
/// to ONE function. THEORY.md §II.1 invariant 1 — typed entry; an
/// unquote target that isn't a symbol is exactly the failure mode the
/// typed-entry template-gate exists to reject. THEORY.md §II.1
/// invariant 2 — free middle; both bytecode AND substitute expansion
/// paths now project through the SAME gate-1 primitive, so a macro
/// that compiles under one strategy compiles under the other (the
/// gate's posture is uniform across the two strategies, no
/// per-strategy drift can creep in).
fn unquote_target_symbol(inner: &Sexp, form: UnquoteForm) -> Result<&str> {
    inner
        .as_symbol()
        .ok_or_else(|| non_symbol_unquote_target(form, inner))
}

/// Gate-2 for the bytecode-template compile path: resolve a template
/// variable name to its index inside the macro's static param list, or
/// raise the structural `LispError::UnboundTemplateVar` rejection. ONE
/// named primitive that the two `compile_node` sites — `Sexp::Unquote(_)`
/// and `Sexp::UnquoteSplice(_)` arms — share. Before this lift the same
/// `params.iter().position(|p| *p == name).ok_or_else(|| unbound_template_var(
/// FORM, name, params))?` projection was inlined twice in one match
/// block; after this lift the two sites collapse to a single
/// `resolve_param_index(name, params, form)?` call and the
/// `Subst(idx)` / `Splice(idx)` ops push from a uniform projection
/// boundary.
///
/// Sibling of `resolve_binding`: the same gate-2 contract on the
/// substitute path. Together the two close the typed-entry template
/// gate's gate-2 (must-be-bound-in-scope) primitive across BOTH
/// expansion strategies — gate-1 (`unquote_target_symbol`) projects the
/// inner to a symbol name; gate-2 looks the name up in the in-scope
/// candidate set. The two paths' candidate sets differ structurally
/// (compile path: `&[&str]` of macro params, returning `usize`;
/// substitute path: `&HashMap<String, Sexp>` of live bindings, returning
/// `&Sexp`), so the gate-2 primitive bifurcates by path — but the
/// rejection shape (`LispError::UnboundTemplateVar { prefix, name, hint }`
/// with `crate::domain::suggest`-driven hint) is identical across both
/// paths. A regression that drifts gate-2's posture (e.g., accepts an
/// unbound `,name` at the bytecode path but not the substitute path) is
/// now a type-level change at this helper, not a silent four-site
/// divergence.
///
/// `form` is `UnquoteForm` — the closed-set typed enum whose two
/// variants are EXACTLY the two reachable syntactic markers
/// (`Unquote` ⊎ `Splice`). Threading the typed marker through the
/// helper boundary (rather than `&'static str`) lands the same
/// compile-time closed-set guarantee `unquote_target_symbol`,
/// `unbound_template_var`, and `non_symbol_unquote_target` carry — a
/// regression that drifts the marker becomes a type error at the call
/// site, not a runtime substring drift.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// inline copies of the gate-2 projection in one match block, paired
/// with the two substitute-path inline copies, is four copies in two
/// functions — past the three-times rule once the structural shape is
/// named. THEORY.md §V.1 — knowable platform; the gate's identity
/// becomes a NAMED primitive consumer-binding rather than a
/// twice-inlined position-and-reject snippet — authoring surfaces
/// (REPL, LSP, `tatara-check`) that want to surface "the typed-entry
/// template-gate rejected your form because the name isn't bound in
/// scope" bind to ONE function per path. THEORY.md §II.1 invariant 1 —
/// typed entry; an unbound template variable is exactly the failure
/// mode the typed-entry template-gate exists to reject. THEORY.md
/// §II.1 invariant 2 — free middle; both expansion strategies'
/// gate-2 emit the SAME structural variant, so a macro that compiles
/// under one strategy compiles under the other.
fn resolve_param_index(name: &str, params: &[&str], form: UnquoteForm) -> Result<usize> {
    params
        .iter()
        .position(|p| *p == name)
        .ok_or_else(|| unbound_template_var(form, name, params))
}

/// Gate-2 for the substitute expansion path: resolve a template
/// variable name to its bound `Sexp` value inside the runtime bindings
/// map, or raise the structural `LispError::UnboundTemplateVar`
/// rejection. ONE named primitive that the two `substitute` sites —
/// the top-level `Sexp::Unquote(_)` arm and the list-inner
/// `Sexp::UnquoteSplice(_)` arm — share. Before this lift the same
/// `bindings.get(sym).<cloned>?.ok_or_else(|| unbound_template_var(
/// FORM, sym, &bound_names(bindings)))` projection was inlined twice
/// across the substitute walker; after this lift the two sites
/// collapse to a single `resolve_binding(bindings, sym, form)?` call
/// (with a trailing `.cloned()` at the top-level arm because that arm
/// returns an owned `Sexp` while the list-inner arm consumes the
/// `&Sexp` borrow directly).
///
/// Sibling of `resolve_param_index`: the same gate-2 contract on the
/// bytecode-template compile path. Together the two close the
/// typed-entry template gate's gate-2 (must-be-bound-in-scope)
/// primitive across BOTH expansion strategies. The candidate set on
/// the substitute path is the live bindings' keys (built fresh per
/// call via `bound_names`) — never a stale snapshot, so the
/// suggest-driven hint is always drawn from the actual in-scope name
/// set the operator sees.
///
/// The returned `&'a Sexp` borrows from `bindings` — the list-inner
/// caller feeds it straight into the `Sexp::List`/`Sexp::Nil`/other
/// splice-expansion match without an intermediate allocation. The
/// top-level caller's owned-Sexp obligation is satisfied by the
/// `.cloned()` projection at the call site, which is a single typed
/// `Sexp::clone` and not a redundant lookup.
///
/// `form` is `UnquoteForm` — same closed-set typed enum threading as
/// `resolve_param_index` and `unquote_target_symbol`. A regression
/// that drifts the marker becomes a type error at the call site, not
/// a runtime substring drift.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// inline copies of the gate-2 projection in the substitute walker,
/// paired with the two compile-path inline copies, is four copies in
/// two functions — past the three-times rule once the structural
/// shape is named. THEORY.md §V.1 — knowable platform; the gate's
/// identity becomes a NAMED primitive consumer-binding rather than a
/// twice-inlined lookup-and-reject snippet. THEORY.md §II.1
/// invariant 1 — typed entry; an unbound template variable is exactly
/// the failure mode the typed-entry template-gate exists to reject.
/// THEORY.md §II.1 invariant 2 — free middle; both expansion
/// strategies' gate-2 emit the SAME structural variant.
fn resolve_binding<'a>(
    bindings: &'a HashMap<String, Sexp>,
    name: &str,
    form: UnquoteForm,
) -> Result<&'a Sexp> {
    bindings
        .get(name)
        .ok_or_else(|| unbound_template_var(form, name, &bound_names(bindings)))
}

/// Compose gate-1 + gate-2 for the bytecode-template compile path into ONE
/// named primitive: project the unquote `inner` to a symbol name
/// (gate-1, via `unquote_target_symbol`) THEN resolve the name to its
/// index inside the macro's static param list (gate-2, via
/// `resolve_param_index`). Sibling of `resolve_unquote_in_bindings`: the
/// same gate-1+gate-2 composition on the substitute expansion path.
///
/// Before this lift, the two `compile_node` arms (`Sexp::Unquote(_)` and
/// `Sexp::UnquoteSplice(_)`) threaded `form: UnquoteForm` through TWO
/// helper calls each — once into `unquote_target_symbol(inner, form)?`
/// (gate-1) AND once into `resolve_param_index(name, params, form)?`
/// (gate-2). The marker's typed identity was re-asserted at the call site
/// twice per arm — four `UnquoteForm::Unquote` / `UnquoteForm::Splice`
/// literal occurrences across the two arms, for what is structurally ONE
/// marker-identity per syntactic-marker arm. After this lift each arm
/// threads the marker ONCE through ONE call, and the gate-1-then-gate-2
/// sequencing lives in the helper body, not at the call site.
///
/// The composition is load-bearing: gate-1 (must-be-a-symbol) MUST fire
/// before gate-2 (must-be-bound-in-scope) — a non-symbol inner is
/// structurally a different failure mode (`LispError::NonSymbolUnquoteTarget`,
/// which carries the offending `SexpWitness`) than an unbound symbol
/// (`LispError::UnboundTemplateVar`, which carries a `name: String` plus
/// a `crate::domain::suggest`-driven hint over the candidate set). A
/// regression that reorders or skips gate-1 would emit
/// `LispError::UnboundTemplateVar { name: "(list 1 2)", ... }` for a
/// non-symbol inner (re-treating the rendered list literal as a bound-
/// name lookup key), which is exactly the diagnostic-confusion this
/// composition exists to rule out. Naming the composition as one
/// primitive makes the sequencing structural — the helper body IS the
/// proof that gate-1 ran before gate-2.
///
/// `form` is `UnquoteForm` — the closed-set typed enum threaded through
/// the composition once and passed onward to both gate-1 and gate-2's
/// rejection-builders. Same posture as `unquote_target_symbol`,
/// `resolve_param_index`, `resolve_binding`, `non_symbol_unquote_target`,
/// and `unbound_template_var` — a regression that drifts the marker
/// becomes a type error at the helper boundary, not a runtime substring
/// drift, AND the marker can no longer drift BETWEEN gate-1 and gate-2
/// at a single call site (which the prior pre-lift shape allowed:
/// `unquote_target_symbol(inner, UnquoteForm::Unquote)?` followed by
/// `resolve_param_index(name, params, UnquoteForm::Splice)?` would
/// type-check but render a misleading diagnostic).
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// gate-1+gate-2 SEQUENCE is itself a named primitive once both halves
/// have been named (two prior runs landed the halves; this run lands the
/// composition). THEORY.md §V.1 — knowable platform; the gate's
/// composition is now load-bearing in the type system — gate-1 cannot be
/// silently skipped, gate-2 cannot be silently reordered before gate-1,
/// and the marker cannot drift between the two halves. THEORY.md §II.1
/// invariant 1 — typed entry; the typed-entry template gate's full
/// rejection chain (non-symbol → unbound-symbol) is now ONE primitive.
/// THEORY.md §II.1 invariant 2 — free middle; both expansion strategies
/// expose the gate's identity as ONE primitive per path, so a macro that
/// passes the gate under one strategy passes under the other (no per-
/// strategy composition drift can creep in).
fn resolve_unquote_in_params(inner: &Sexp, params: &[&str], form: UnquoteForm) -> Result<usize> {
    let name = unquote_target_symbol(inner, form)?;
    resolve_param_index(name, params, form)
}

/// Compose gate-1 + gate-2 for the substitute expansion path into ONE
/// named primitive: project the unquote `inner` to a symbol name
/// (gate-1, via `unquote_target_symbol`) THEN resolve the name to its
/// bound `Sexp` value inside the runtime bindings map (gate-2, via
/// `resolve_binding`). Sibling of `resolve_unquote_in_params`: the same
/// gate-1+gate-2 composition on the bytecode-template compile path.
///
/// Before this lift, the substitute walker's two unquote sites (the
/// top-level `Sexp::Unquote(_)` arm and the list-inner
/// `Sexp::UnquoteSplice(_)` arm) threaded `form: UnquoteForm` through
/// TWO helper calls each — once into `unquote_target_symbol(inner,
/// form)?` (gate-1) AND once into `resolve_binding(bindings, name,
/// form)?` (gate-2). After this lift each site threads the marker
/// ONCE through ONE call. Same composition contract as
/// `resolve_unquote_in_params` — gate-1 fires before gate-2 by the
/// helper body's `?`-then-call sequencing, NOT by call-site discipline.
///
/// The returned `&'a Sexp` borrows from `bindings` so the list-inner
/// caller feeds it straight into the `Sexp::List`/`Sexp::Nil`/other
/// splice-expansion match without an intermediate allocation; the
/// top-level caller's owned-Sexp obligation is satisfied by a
/// `.cloned()` projection at the call site (one typed `Sexp::clone`,
/// no redundant lookup).
///
/// `form` is `UnquoteForm` — same closed-set typed enum threading as
/// `resolve_unquote_in_params` and all the helpers it composes. After
/// this lift, the marker's identity flows through the substitute path's
/// typed-entry template gate via ONE explicit pass per call site, not
/// two; the gate's gate-1+gate-2 sequencing is structural across both
/// expansion strategies.
///
/// Theory anchor: same as `resolve_unquote_in_params`. THEORY.md §VI.1
/// (generation over composition; named composition of named gates),
/// THEORY.md §V.1 (knowable platform; gate composition is type-system
/// load-bearing), THEORY.md §II.1 invariant 1 (typed entry; the full
/// rejection chain is ONE primitive), THEORY.md §II.1 invariant 2
/// (free middle; both strategies share the same composition shape).
fn resolve_unquote_in_bindings<'a>(
    inner: &Sexp,
    bindings: &'a HashMap<String, Sexp>,
    form: UnquoteForm,
) -> Result<&'a Sexp> {
    let name = unquote_target_symbol(inner, form)?;
    resolve_binding(bindings, name, form)
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
        got: got.map(crate::domain::sexp_witness),
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
/// through `crate::domain::sexp_witness(_)` — the typed joint
/// primitive that pairs the offending `Sexp`'s `SexpShape` with its
/// `Sexp::Display` projection in ONE owned `SexpWitness` value, so
/// authoring tools bind to both the structural shape AND the rendered
/// literal across the variant slot. Same posture as `non_symbol_param`,
/// `non_symbol_unquote_target`, `splice_outside_list`, and
/// `defmacro_non_symbol_name`'s helpers after the typed-witness
/// promotion of their `got` slots.
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
        got: crate::domain::sexp_witness(got),
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
            params: MacroParams {
                required: vec!["x".into()],
                rest: None,
            },
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

    // ── unquote_target_symbol: typed gate-1 primitive for ,X / ,@X ──────
    //
    // The `unquote_target_symbol(inner, form)?` primitive lifts the
    // inline `inner.as_symbol().ok_or_else(|| non_symbol_unquote_target(
    // form, inner))?` pattern that previously appeared at four call
    // sites (`compile_node` Unquote/UnquoteSplice + `substitute` Unquote
    // + `substitute` list-inner UnquoteSplice) behind ONE named
    // primitive. The tests below pin: (a) the Ok-arm borrows the
    // symbol name from `inner` for both UnquoteForm variants; (b) the
    // Err-arm routes through `non_symbol_unquote_target` and emits the
    // structural `LispError::NonSymbolUnquoteTarget` variant carrying
    // the typed `SexpWitness` (joint shape + display identity) for the
    // closed set of reachable non-symbol shapes (int / keyword / list /
    // nil); (c) the helper is path-uniform — the same Ok / Err
    // contracts hold regardless of which call site invokes it. A
    // regression that re-inlines the gate-1 projection at any of the
    // four call sites can no longer drift independent of the others —
    // the helper IS the gate.

    #[test]
    fn unquote_target_symbol_returns_symbol_for_symbol_inner_under_unquote() {
        // Positive control for the Ok-arm: `inner = Sexp::Symbol("xs")`
        // under `UnquoteForm::Unquote` projects through `as_symbol()`
        // to the borrowed `&str`. The returned slice's lifetime is
        // tied to `inner` so the caller can feed it directly into
        // `params.iter().position(...)` (`compile_node`) or
        // `bindings.get(...)` (`substitute`) without an intermediate
        // allocation. Fail-before/pass-after: this assert is meaningless
        // pre-lift because the helper does not exist; post-lift it
        // pins the typed gate-1 contract at the named primitive.
        let inner = Sexp::symbol("xs");
        let name = unquote_target_symbol(&inner, UnquoteForm::Unquote)
            .expect("symbol inner must project to Ok");
        assert_eq!(name, "xs");
    }

    #[test]
    fn unquote_target_symbol_returns_symbol_for_symbol_inner_under_splice() {
        // Sibling positive control: `UnquoteForm::Splice` shares the
        // gate-1 contract with `Unquote`. The helper is path-uniform
        // across both syntactic markers — a regression that bifurcates
        // the two arms (e.g., accepting non-symbols for `,@X` but not
        // `,X`) fails-loudly here. Pins that the closed-set
        // `UnquoteForm` enum's two variants share ONE projection
        // posture across the gate-1 boundary.
        let inner = Sexp::symbol("rest");
        let name = unquote_target_symbol(&inner, UnquoteForm::Splice)
            .expect("symbol inner must project to Ok under Splice");
        assert_eq!(name, "rest");
    }

    #[test]
    fn unquote_target_symbol_rejects_int_inner_under_unquote() {
        // Negative control for the Err-arm: `inner = Sexp::Int(5)` is
        // NOT a symbol — the gate-1 projection fires and routes through
        // `non_symbol_unquote_target` to the structural
        // `LispError::NonSymbolUnquoteTarget` variant. Pin the variant
        // identity AND the typed `SexpWitness` joint identity (shape +
        // display literal): a regression that drops the witness shape
        // or display fails-loudly here.
        let inner = Sexp::int(5);
        let err = unquote_target_symbol(&inner, UnquoteForm::Unquote)
            .expect_err("int inner must error at gate-1");
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => {
                assert_eq!(prefix, UnquoteForm::Unquote);
                assert_eq!(got.shape, crate::error::SexpShape::Int);
                assert_eq!(got.display, "5");
            }
            other => panic!("expected NonSymbolUnquoteTarget, got: {other:?}"),
        }
    }

    #[test]
    fn unquote_target_symbol_rejects_list_inner_under_splice() {
        // Sibling negative control: `inner = (list 1 2)` is a list, not
        // a symbol — the gate-1 projection fires AND routes through
        // `non_symbol_unquote_target(UnquoteForm::Splice, inner)`. Pins
        // both the variant identity AND the typed witness's joint
        // shape (`SexpShape::List`) + display (`"(list 1 2)"`) so a
        // future shape drift fails-loudly. Sibling of the Int / Unquote
        // pin: closes the gate-1 contract across the closed-set
        // product of {Int, List, Keyword, …} × {Unquote, Splice}.
        let inner = Sexp::List(vec![Sexp::symbol("list"), Sexp::int(1), Sexp::int(2)]);
        let err = unquote_target_symbol(&inner, UnquoteForm::Splice)
            .expect_err("list inner must error at gate-1");
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => {
                assert_eq!(prefix, UnquoteForm::Splice);
                assert_eq!(got.shape, crate::error::SexpShape::List);
                assert_eq!(got.display, "(list 1 2)");
            }
            other => panic!("expected NonSymbolUnquoteTarget, got: {other:?}"),
        }
    }

    #[test]
    fn unquote_target_symbol_rejects_keyword_inner_with_typed_witness() {
        // Pin a third reachable non-symbol shape: `Sexp::Keyword(":foo")`.
        // The gate-1 projection rejects keywords AS WELL as ints and
        // lists — closes the closed-set of "non-symbol shapes the gate
        // rejects" across one more reachable variant. The typed witness
        // carries `SexpShape::Keyword` + display `:foo` jointly so
        // authoring tools (REPL, LSP) bind on the structural shape
        // directly.
        let inner = Sexp::keyword("foo");
        let err = unquote_target_symbol(&inner, UnquoteForm::Unquote)
            .expect_err("keyword inner must error at gate-1");
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => {
                assert_eq!(prefix, UnquoteForm::Unquote);
                assert_eq!(got.shape, crate::error::SexpShape::Keyword);
                assert_eq!(got.display, ":foo");
            }
            other => panic!("expected NonSymbolUnquoteTarget, got: {other:?}"),
        }
    }

    #[test]
    fn unquote_target_symbol_consolidates_four_inline_callsites_into_one_helper() {
        // Path-uniformity pin: end-to-end through ALL FOUR call sites
        // (`compile_node` Unquote, `compile_node` UnquoteSplice,
        // `substitute` Unquote, `substitute` list-inner UnquoteSplice)
        // every non-symbol unquote target now routes through the SAME
        // `unquote_target_symbol(inner, form)?` helper. The four
        // end-to-end expansions below all reject with the SAME variant
        // (`NonSymbolUnquoteTarget`) — pins that the lift preserves the
        // path-uniform rejection contract `non_symbol_unquote_target`'s
        // prior lift established (and that drove the bytecode-vs-
        // substitute reunification in 0e9c… and successors). A
        // regression that re-inlines the gate-1 projection at one of
        // the four sites can drift the four call sites independent of
        // each other — this test would catch that drift.
        let cases: &[(&str, UnquoteForm)] = &[
            // compile_node Unquote (bytecode-path)
            ("(defmacro w (x) `,(list 1 2)) (w 1)", UnquoteForm::Unquote),
            // compile_node UnquoteSplice (bytecode-path)
            ("(defmacro w (x) `(list ,@5)) (w 1)", UnquoteForm::Splice),
        ];
        for (src, expected_form) in cases {
            let mut e = Expander::new();
            let err = e
                .expand_program(read(src).unwrap())
                .expect_err("non-symbol unquote target must error end-to-end");
            match err {
                LispError::NonSymbolUnquoteTarget { prefix, .. } => {
                    assert_eq!(prefix, *expected_form, "for src: {src}");
                }
                other => panic!("expected NonSymbolUnquoteTarget for {src}, got: {other:?}"),
            }
        }
        // substitute Unquote (substitute-only path) — sibling pin to
        // `non_symbol_unquote_in_substitute_emits_structural_variant`.
        let mut e_subst = Expander::new_substitute_only();
        let err = e_subst
            .expand_program(read("(defmacro w (x) `,(list 1 2)) (w 1)").unwrap())
            .expect_err("substitute Unquote must error end-to-end");
        assert!(
            matches!(
                err,
                LispError::NonSymbolUnquoteTarget {
                    prefix: UnquoteForm::Unquote,
                    ..
                }
            ),
            "expected NonSymbolUnquoteTarget at substitute Unquote, got: {err:?}"
        );
        // substitute list-inner UnquoteSplice (substitute-only path) —
        // sibling pin to
        // `non_symbol_unquote_splice_inside_list_in_substitute_emits_…`.
        let mut e_subst2 = Expander::new_substitute_only();
        let err = e_subst2
            .expand_program(read("(defmacro w (x) `(outer ,@(list 1 2))) (w 1)").unwrap())
            .expect_err("substitute UnquoteSplice-in-list must error end-to-end");
        assert!(
            matches!(
                err,
                LispError::NonSymbolUnquoteTarget {
                    prefix: UnquoteForm::Splice,
                    ..
                }
            ),
            "expected NonSymbolUnquoteTarget at substitute UnquoteSplice-in-list, got: {err:?}"
        );
    }

    // ── Gate-2 (must-be-bound-in-scope) typed primitives ──────────────
    // Pins the contract of the two gate-2 helpers — `resolve_param_index`
    // (bytecode-template compile path) and `resolve_binding`
    // (substitute path) — that the four inline `<lookup>.ok_or_else(||
    // unbound_template_var(FORM, name, candidates))` projections at
    // `compile_node` Unquote/UnquoteSplice AND `substitute` Unquote/
    // UnquoteSplice-inside-list collapse behind. Tests pin: (a) Ok-arm
    // projection under both `UnquoteForm` variants — the helper returns
    // the resolved `usize` (compile path) or `&Sexp` (substitute path)
    // for in-scope names; (b) Err-arm projection routes through
    // `unbound_template_var` to the typed `LispError::UnboundTemplateVar`
    // variant with the correct `prefix` AND the suggest-driven `hint`;
    // (c) the helpers are path-uniform — both compile-path arms share
    // ONE `resolve_param_index`; both substitute-path arms share ONE
    // `resolve_binding`. A regression that re-inlines the gate-2
    // projection at any of the four call sites can no longer drift
    // independent of the others — the two helpers ARE the gate.

    #[test]
    fn resolve_param_index_returns_position_for_bound_name_under_unquote() {
        // Positive control for the Ok-arm: `name = "x"` against
        // `params = ["a", "x", "rest"]` projects through
        // `params.iter().position(|p| *p == name)` to `Some(1)`, which
        // the helper unwraps to `Ok(1)`. The returned index feeds
        // directly into `TemplateOp::Subst(idx)` at the compile site.
        let params = ["a", "x", "rest"];
        let idx = resolve_param_index("x", &params, UnquoteForm::Unquote)
            .expect("bound name must project to Ok at gate-2");
        assert_eq!(idx, 1);
    }

    #[test]
    fn resolve_param_index_returns_position_for_bound_name_under_splice() {
        // Sibling positive control: `UnquoteForm::Splice` shares the
        // gate-2 contract with `Unquote`. The helper is path-uniform
        // across both syntactic markers on the compile path — a
        // regression that bifurcates the two arms fails-loudly here.
        let params = ["a", "x", "rest"];
        let idx = resolve_param_index("rest", &params, UnquoteForm::Splice)
            .expect("bound name must project to Ok at gate-2 under Splice");
        assert_eq!(idx, 2);
    }

    #[test]
    fn resolve_param_index_rejects_unbound_name_with_hint_under_unquote() {
        // Negative control for the Err-arm: `name = "xs"` against
        // `params = ["x"]` — distance 1, bound 1 — routes through
        // `unbound_template_var` to the structural
        // `LispError::UnboundTemplateVar` variant with `hint = Some("x")`.
        // Pin the variant identity AND the prefix AND the suggest-driven
        // hint: a regression that drops the suggestion fails-loudly here.
        let params = ["x"];
        let err = resolve_param_index("xs", &params, UnquoteForm::Unquote)
            .expect_err("unbound name must error at gate-2");
        match err {
            LispError::UnboundTemplateVar { prefix, name, hint } => {
                assert_eq!(prefix, UnquoteForm::Unquote);
                assert_eq!(name, "xs");
                assert_eq!(hint.as_deref(), Some("x"));
            }
            other => panic!("expected UnboundTemplateVar, got: {other:?}"),
        }
    }

    #[test]
    fn resolve_param_index_rejects_unbound_name_without_hint_under_splice() {
        // Sibling negative control: `name = "wholly-unrelated"` against
        // `params = ["x"]` — past the bounded edit distance, so no hint.
        // Pin that the suggest-driven hint stays empty under Splice when
        // the substrate isn't confident — a wrong hint is worse than no
        // hint. Closes the closed-set product of {hint, no-hint} ×
        // {Unquote, Splice} on the compile-path gate-2.
        let params = ["x"];
        let err = resolve_param_index("wholly-unrelated", &params, UnquoteForm::Splice)
            .expect_err("unrelated unbound must error at gate-2");
        match err {
            LispError::UnboundTemplateVar { prefix, name, hint } => {
                assert_eq!(prefix, UnquoteForm::Splice);
                assert_eq!(name, "wholly-unrelated");
                assert_eq!(hint, None);
            }
            other => panic!("expected UnboundTemplateVar, got: {other:?}"),
        }
    }

    #[test]
    fn resolve_binding_returns_value_for_bound_name_under_unquote() {
        // Positive control for the substitute-path Ok-arm: `name = "x"`
        // against a bindings map `{x: 42, y: "hi"}` projects through
        // `bindings.get(name)` to `Some(&Sexp::Int(42))`, which the
        // helper unwraps to `Ok(&Sexp::Int(42))`. The returned
        // `&Sexp` borrows from the bindings map — the top-level
        // `Sexp::Unquote(_)` substitute caller adds a single
        // `.cloned()` to satisfy its owned-`Sexp` return obligation.
        let mut bindings: HashMap<String, Sexp> = HashMap::new();
        bindings.insert("x".to_string(), Sexp::int(42));
        bindings.insert("y".to_string(), Sexp::string("hi"));
        let val = resolve_binding(&bindings, "x", UnquoteForm::Unquote)
            .expect("bound name must project to Ok at gate-2 (substitute)");
        assert_eq!(val, &Sexp::int(42));
    }

    #[test]
    fn resolve_binding_returns_value_for_bound_name_under_splice() {
        // Sibling positive control: `UnquoteForm::Splice` shares the
        // gate-2 contract with `Unquote` on the substitute path too.
        // The bound value is a `Sexp::List` because the splice arm's
        // caller match expression expects `Sexp::List(items)` — but
        // the helper itself doesn't inspect the value's shape; it
        // just hands back the borrow. A regression that gate-checks
        // the value's shape inside `resolve_binding` (instead of at
        // the caller match arm) fails-loudly here.
        let mut bindings: HashMap<String, Sexp> = HashMap::new();
        bindings.insert(
            "args".to_string(),
            Sexp::List(vec![Sexp::int(1), Sexp::int(2)]),
        );
        let val = resolve_binding(&bindings, "args", UnquoteForm::Splice)
            .expect("bound name must project to Ok at gate-2 under Splice");
        assert_eq!(val, &Sexp::List(vec![Sexp::int(1), Sexp::int(2)]));
    }

    #[test]
    fn resolve_binding_rejects_unbound_name_with_hint_under_unquote() {
        // Negative control for the substitute-path Err-arm: `name =
        // "xs"` against bindings `{x: 1}` — distance 1, bound 1 —
        // routes through `unbound_template_var` to the structural
        // `LispError::UnboundTemplateVar` variant with `hint =
        // Some("x")`. The candidate set is drawn from
        // `bound_names(bindings)` — the live bindings' keys, never a
        // stale snapshot.
        let mut bindings: HashMap<String, Sexp> = HashMap::new();
        bindings.insert("x".to_string(), Sexp::int(1));
        let err = resolve_binding(&bindings, "xs", UnquoteForm::Unquote)
            .expect_err("unbound name must error at gate-2 (substitute)");
        match err {
            LispError::UnboundTemplateVar { prefix, name, hint } => {
                assert_eq!(prefix, UnquoteForm::Unquote);
                assert_eq!(name, "xs");
                assert_eq!(hint.as_deref(), Some("x"));
            }
            other => panic!("expected UnboundTemplateVar, got: {other:?}"),
        }
    }

    #[test]
    fn resolve_binding_rejects_unbound_name_without_hint_under_splice() {
        // Sibling negative control on the substitute path: past-bound
        // distance → no hint. Closes the closed-set product of
        // {hint, no-hint} × {Unquote, Splice} on the substitute-path
        // gate-2.
        let mut bindings: HashMap<String, Sexp> = HashMap::new();
        bindings.insert("args".to_string(), Sexp::Nil);
        let err = resolve_binding(&bindings, "wholly-unrelated", UnquoteForm::Splice)
            .expect_err("unrelated unbound must error at gate-2");
        match err {
            LispError::UnboundTemplateVar { prefix, name, hint } => {
                assert_eq!(prefix, UnquoteForm::Splice);
                assert_eq!(name, "wholly-unrelated");
                assert_eq!(hint, None);
            }
            other => panic!("expected UnboundTemplateVar, got: {other:?}"),
        }
    }

    #[test]
    fn gate_2_consolidates_four_inline_callsites_into_two_helpers() {
        // Path-uniformity pin: end-to-end through ALL FOUR call sites
        // (`compile_node` Unquote, `compile_node` UnquoteSplice,
        // `substitute` Unquote, `substitute` list-inner UnquoteSplice)
        // every unbound-template-var rejection now routes through one
        // of the TWO `resolve_param_index` / `resolve_binding` helpers
        // — `Expander::new()` runs the compile path, so its two arms
        // share `resolve_param_index`; `Expander::new_substitute_only()`
        // runs the substitute path, so its two arms share
        // `resolve_binding`. The four end-to-end expansions below all
        // reject with the SAME variant (`UnboundTemplateVar`) with the
        // expected `prefix` — pins that the lift preserves the
        // path-uniform rejection contract `unbound_template_var`'s
        // prior naming established. A regression that re-inlines the
        // gate-2 projection at one of the four sites can drift the
        // four call sites independent of each other — this test would
        // catch that drift.
        struct Case {
            src: &'static str,
            expander: fn() -> Expander,
            expected_form: UnquoteForm,
        }
        let cases: &[Case] = &[
            // compile_node Unquote (bytecode path) — uses resolve_param_index
            Case {
                src: "(defmacro w (x) `(list ,xs)) (w 1)",
                expander: Expander::new,
                expected_form: UnquoteForm::Unquote,
            },
            // compile_node UnquoteSplice (bytecode path) — uses resolve_param_index
            Case {
                src: "(defmacro call (f &rest args) `(,f ,@argz)) (call foo a b)",
                expander: Expander::new,
                expected_form: UnquoteForm::Splice,
            },
            // substitute Unquote (substitute-only path) — uses resolve_binding
            Case {
                src: "(defmacro w (x) `(list ,xs)) (w 1)",
                expander: Expander::new_substitute_only,
                expected_form: UnquoteForm::Unquote,
            },
            // substitute UnquoteSplice-in-list (substitute-only path) — uses resolve_binding
            Case {
                src: "(defmacro call (f &rest args) `(,f ,@argz)) (call foo a b)",
                expander: Expander::new_substitute_only,
                expected_form: UnquoteForm::Splice,
            },
        ];
        for case in cases {
            let mut e = (case.expander)();
            let err = e
                .expand_program(read(case.src).unwrap())
                .expect_err("unbound template var must error end-to-end");
            match err {
                LispError::UnboundTemplateVar { prefix, .. } => {
                    assert_eq!(prefix, case.expected_form, "for src: {}", case.src);
                }
                other => panic!(
                    "expected UnboundTemplateVar for {}, got: {other:?}",
                    case.src
                ),
            }
        }
    }

    // ── resolve_unquote_in_params / _in_bindings: gate-1+gate-2 composition ─

    #[test]
    fn resolve_unquote_in_params_returns_index_for_symbol_inner_under_unquote() {
        // Ok-arm composition under `UnquoteForm::Unquote`: gate-1 projects
        // the symbol-inner to "x"; gate-2 looks "x" up in `params` and
        // returns its index. The combined helper returns the gate-2
        // result directly — pins that gate-1's Ok-arm threads into
        // gate-2's input without intermediate state.
        let inner = Sexp::symbol("x");
        let params = ["x", "y"];
        let idx = resolve_unquote_in_params(&inner, &params, UnquoteForm::Unquote)
            .expect("symbol-inner bound at index 0 must resolve");
        assert_eq!(idx, 0);
    }

    #[test]
    fn resolve_unquote_in_params_returns_index_for_symbol_inner_under_splice() {
        // Sibling Ok-arm under `UnquoteForm::Splice`: pins that the
        // marker doesn't change the projection — only the rejection
        // path's `prefix` slot.
        let inner = Sexp::symbol("args");
        let params = ["f", "args"];
        let idx = resolve_unquote_in_params(&inner, &params, UnquoteForm::Splice)
            .expect("symbol-inner bound at index 1 must resolve");
        assert_eq!(idx, 1);
    }

    #[test]
    fn resolve_unquote_in_params_rejects_non_symbol_inner_at_gate_1() {
        // Err-arm at gate-1 (must-be-a-symbol): the inner is a list, not
        // a symbol, so gate-1 rejects via `non_symbol_unquote_target`
        // BEFORE gate-2's param lookup runs. Pins that the composition's
        // sequencing is gate-1-then-gate-2: a regression that runs
        // gate-2 first would attempt to look up "(list 1 2)" as a param
        // name and emit `LispError::UnboundTemplateVar { name: "(list 1
        // 2)", ... }` — a confusing diagnostic that would substring-grep
        // "unbound" instead of "expected symbol". This test pins the
        // structural floor: a non-symbol inner is rejected as a non-
        // symbol, never re-treated as a bound-name lookup key.
        let inner = Sexp::List(vec![Sexp::symbol("list"), Sexp::int(1), Sexp::int(2)]);
        let params = ["x"];
        let err = resolve_unquote_in_params(&inner, &params, UnquoteForm::Unquote)
            .expect_err("non-symbol inner must reject at gate-1");
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => {
                assert_eq!(prefix, UnquoteForm::Unquote);
                assert_eq!(got.display, "(list 1 2)");
            }
            other => panic!("expected NonSymbolUnquoteTarget (gate-1), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_unquote_in_params_rejects_unbound_symbol_at_gate_2() {
        // Err-arm at gate-2 (must-be-bound-in-scope): the inner IS a
        // symbol (gate-1 passes) but the name isn't in `params`, so
        // gate-2 rejects via `unbound_template_var`. Pins that gate-1
        // forwards its Ok-arm `&str` borrow into gate-2's lookup, and
        // that the marker `prefix` is threaded into gate-2's rejection
        // unchanged (a regression that hard-codes `UnquoteForm::Unquote`
        // at the composition boundary would fail this Splice-marker
        // assertion).
        let inner = Sexp::symbol("missing");
        let params = ["x", "y"];
        let err = resolve_unquote_in_params(&inner, &params, UnquoteForm::Splice)
            .expect_err("unbound symbol must reject at gate-2");
        match err {
            LispError::UnboundTemplateVar { prefix, name, .. } => {
                assert_eq!(prefix, UnquoteForm::Splice);
                assert_eq!(name, "missing");
            }
            other => panic!("expected UnboundTemplateVar (gate-2), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_unquote_in_bindings_returns_borrow_for_symbol_inner_under_unquote() {
        // Substitute-path sibling of `resolve_unquote_in_params_returns_
        // index_for_symbol_inner_under_unquote`. The combined helper
        // composes gate-1 (project inner to symbol) THEN gate-2 (look
        // up name in bindings). The returned `&Sexp` borrows from
        // `bindings` so the list-inner caller threads it straight into
        // the splice-expansion match without an intermediate allocation.
        let mut bindings: HashMap<String, Sexp> = HashMap::new();
        bindings.insert("v".to_string(), Sexp::int(42));
        let inner = Sexp::symbol("v");
        let val = resolve_unquote_in_bindings(&inner, &bindings, UnquoteForm::Unquote)
            .expect("symbol-inner bound to 42 must resolve");
        assert_eq!(val, &Sexp::int(42));
    }

    #[test]
    fn resolve_unquote_in_bindings_rejects_non_symbol_inner_at_gate_1() {
        // Substitute-path sibling of `resolve_unquote_in_params_rejects_
        // non_symbol_inner_at_gate_1`. Pins the gate-1-then-gate-2
        // sequencing on the substitute path: a non-symbol inner is
        // rejected as a non-symbol BEFORE the bindings map is consulted.
        let bindings: HashMap<String, Sexp> = HashMap::new();
        let inner = Sexp::int(5);
        let err = resolve_unquote_in_bindings(&inner, &bindings, UnquoteForm::Splice)
            .expect_err("non-symbol inner must reject at gate-1");
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => {
                assert_eq!(prefix, UnquoteForm::Splice);
                assert_eq!(got.display, "5");
            }
            other => panic!("expected NonSymbolUnquoteTarget (gate-1), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_unquote_in_bindings_rejects_unbound_symbol_at_gate_2() {
        // Substitute-path sibling of `resolve_unquote_in_params_rejects_
        // unbound_symbol_at_gate_2`. Pins the gate-2 rejection on the
        // substitute path with the marker threaded into the rejection's
        // `prefix` slot.
        let mut bindings: HashMap<String, Sexp> = HashMap::new();
        bindings.insert("known".to_string(), Sexp::Nil);
        let inner = Sexp::symbol("missing");
        let err = resolve_unquote_in_bindings(&inner, &bindings, UnquoteForm::Unquote)
            .expect_err("unbound symbol must reject at gate-2");
        match err {
            LispError::UnboundTemplateVar { prefix, name, .. } => {
                assert_eq!(prefix, UnquoteForm::Unquote);
                assert_eq!(name, "missing");
            }
            other => panic!("expected UnboundTemplateVar (gate-2), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_unquote_helpers_consolidate_four_inline_gate12_sites() {
        // End-to-end pin: all FOUR call sites of the gate-1+gate-2
        // composition (compile_node Unquote, compile_node UnquoteSplice,
        // substitute Unquote, substitute list-inner UnquoteSplice) now
        // share TWO composed primitives — `resolve_unquote_in_params`
        // on the bytecode path, `resolve_unquote_in_bindings` on the
        // substitute path — and ALL four reject gate-1 failures (non-
        // symbol inner) with the SAME `LispError::NonSymbolUnquoteTarget`
        // variant carrying the expected `prefix` slot. Before the lift,
        // each site threaded `form` twice through two helper calls; this
        // test pins that the lift preserves the gate's rejection-shape
        // identity across all four sites for a non-symbol inner — i.e.
        // gate-1 fires identically across both expansion strategies.
        struct Case {
            src: &'static str,
            expander: fn() -> Expander,
            expected_form: UnquoteForm,
        }
        let cases: &[Case] = &[
            Case {
                src: "(defmacro w (x) `,(list 1 2)) (w 1)",
                expander: Expander::new,
                expected_form: UnquoteForm::Unquote,
            },
            Case {
                src: "(defmacro w (x) `(outer ,@5)) (w 1)",
                expander: Expander::new,
                expected_form: UnquoteForm::Splice,
            },
            Case {
                src: "(defmacro w (x) `,(list 1 2)) (w 1)",
                expander: Expander::new_substitute_only,
                expected_form: UnquoteForm::Unquote,
            },
            Case {
                src: "(defmacro w (x) `(outer ,@(list 1 2))) (w 1)",
                expander: Expander::new_substitute_only,
                expected_form: UnquoteForm::Splice,
            },
        ];
        for case in cases {
            let mut e = (case.expander)();
            let err = e
                .expand_program(read(case.src).unwrap())
                .expect_err("non-symbol inner must error end-to-end");
            match err {
                LispError::NonSymbolUnquoteTarget { prefix, .. } => {
                    assert_eq!(prefix, case.expected_form, "for src: {}", case.src);
                }
                other => panic!(
                    "expected NonSymbolUnquoteTarget for {}, got: {other:?}",
                    case.src
                ),
            }
        }
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

    // ── splice_value_into: the shared splice-result coercion ──

    #[test]
    fn splice_value_into_list_flattens_elements_into_builder() {
        // The canonical splice arm: a bound LIST contributes its elements
        // in order, preserving anything already in the builder.
        let mut builder = vec![Sexp::symbol("outer")];
        splice_value_into(&mut builder, &Sexp::List(vec![Sexp::int(1), Sexp::int(2)]));
        assert_eq!(
            builder,
            vec![Sexp::symbol("outer"), Sexp::int(1), Sexp::int(2)]
        );
    }

    #[test]
    fn splice_value_into_nil_is_a_noop() {
        // Splicing the empty list (`Sexp::Nil`) contributes nothing —
        // the builder is unchanged.
        let mut builder = vec![Sexp::symbol("outer")];
        splice_value_into(&mut builder, &Sexp::Nil);
        assert_eq!(builder, vec![Sexp::symbol("outer")]);
    }

    #[test]
    fn splice_value_into_scalar_pushes_single_element() {
        // A non-list, non-nil bound value degrades `,@x` to `,x`: it
        // splices as exactly one element. Pins the "free middle" coercion
        // every scalar shape (int, keyword, …) shares.
        let mut builder = vec![Sexp::symbol("outer")];
        splice_value_into(&mut builder, &Sexp::int(5));
        assert_eq!(builder, vec![Sexp::symbol("outer"), Sexp::int(5)]);
        let mut other: Vec<Sexp> = vec![];
        splice_value_into(&mut other, &Sexp::keyword("k"));
        assert_eq!(other, vec![Sexp::keyword("k")]);
    }

    #[test]
    fn splice_of_non_list_value_coerces_identically_under_both_paths() {
        // The point of the lift: the NON-list splice arms (scalar → single
        // element, nil → nothing) coerce identically under the substitute
        // AND bytecode strategies. Before the coercion was lifted to ONE
        // primitive these two arms lived inline at two sites; this test
        // pins that the two strategies cannot drift on the non-list arms.
        let scalar = "(defmacro f (x) `(outer ,@x)) (f 5)";
        let empty = "(defmacro g (x) `(outer ,@x)) (g ())";
        for src in [scalar, empty] {
            let mut subst = Expander::new_substitute_only();
            let mut bytecode = Expander::new();
            let out_subst = subst.expand_program(read(src).unwrap()).unwrap();
            let out_byte = bytecode.expand_program(read(src).unwrap()).unwrap();
            assert_eq!(out_subst, out_byte, "strategies must agree for {src}");
        }
        let mut e = Expander::new();
        assert_eq!(
            e.expand_program(read(scalar).unwrap()).unwrap()[0],
            parse("(outer 5)")
        );
        let mut e2 = Expander::new();
        assert_eq!(
            e2.expand_program(read(empty).unwrap()).unwrap()[0],
            parse("(outer)")
        );
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
                (*rest_position, got.as_ref().map(|w| w.display.as_str()))
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
            got: Some(crate::error::SexpWitness::new(
                crate::error::SexpShape::Int,
                "5",
            )),
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
        // match. The `got: &Sexp` parameter rides through
        // `crate::domain::sexp_witness` into the variant's typed
        // `got: SexpWitness` slot so BOTH the structural shape AND
        // the rendered literal are preserved across the helper
        // boundary, parallel to how `defmacro_non_symbol_name`,
        // `non_symbol_param`, and `non_symbol_unquote_target` project
        // their `&Sexp` arguments through the same typed joint
        // primitive.
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
                    assert_eq!(g.shape, crate::error::SexpShape::Symbol);
                    assert_eq!(g.display, "x");
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
            LispError::DefmacroNonListParams { head, got } => (*head, got.display.as_str()),
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
        let err = apply_compiled("test-macro", &MacroParams::default(), &tmpl, &[])
            .expect_err("bad idx must error");
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
        let err = apply_compiled("call-macro", &MacroParams::default(), &tmpl, &[])
            .expect_err("bad splice idx must error");
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
        let err = apply_compiled("test-macro", &MacroParams::default(), &tmpl, &[])
            .expect_err("bad idx must error");
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
        let err = apply_compiled("call-macro", &MacroParams::default(), &tmpl, &[])
            .expect_err("bad splice idx must error");
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
        // Negative control: the `missing_macro_arg` gate in the shared
        // positional binder (`MacroParams::bind`) fires BEFORE the bytecode
        // loop runs,
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

    // ── MacroParams: the typed param-list primitive ─────────────────────
    //
    // `parse_params` now yields a `MacroParams { required, rest }` whose
    // shape makes "&rest is last + at-most-one" structural rather than a
    // construction discipline a `Vec<Param>` only happened to uphold. These
    // tests pin the parser's mapping into the typed shape, the flat-index
    // contract `names()` exposes to the template bytecode, and the single
    // positional binder `bind()` both expansion strategies now route
    // through. The end-to-end `rest_param_splices_with_at` and
    // `compiled_template_matches_substitute_path` tests above are the
    // path-uniformity guards proving both strategies still agree.

    #[test]
    fn parse_params_maps_required_then_rest_into_typed_shape() {
        // `(a b &rest c)` — two required, one rest. The rest name lands in
        // the `Option`, never in `required`.
        let params = parse_params(&read("a b &rest c").unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: vec!["a".into(), "b".into()],
                rest: Some("c".into()),
            }
        );
    }

    #[test]
    fn parse_params_rest_absent_leaves_none() {
        // `(x y)` — no `&rest`, so `rest` is structurally `None`. There is
        // no representation in which a rest-less list carries a stray rest.
        let params = parse_params(&read("x y").unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: vec!["x".into(), "y".into()],
                rest: None,
            }
        );
    }

    #[test]
    fn names_are_required_then_rest_in_flat_index_order() {
        // The flat-index contract the bytecode `Subst(idx)`/`Splice(idx)`
        // depends on: required names in order at 0.., then the rest name at
        // index `required.len()`.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            rest: Some("c".into()),
        };
        assert_eq!(params.names(), vec!["a", "b", "c"]);
        assert_eq!(params.names()[params.required.len()], "c");
    }

    #[test]
    fn bind_threads_required_positionally_and_collects_rest_as_list() {
        // `(a b &rest c)` bound to `1 2 3 4`: a=1, b=2, c=(3 4). The bound
        // vec is parallel to `names()`, so the rest list sits at the rest's
        // flat index.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            rest: Some("c".into()),
        };
        let vals = params
            .bind(
                "m",
                &[Sexp::int(1), Sexp::int(2), Sexp::int(3), Sexp::int(4)],
            )
            .unwrap();
        assert_eq!(
            vals,
            vec![
                Sexp::int(1),
                Sexp::int(2),
                Sexp::List(vec![Sexp::int(3), Sexp::int(4)]),
            ]
        );
    }

    #[test]
    fn bind_rest_with_no_remaining_args_is_the_empty_list() {
        // Exactly-saturated required args + a rest that captures nothing →
        // the rest binds to the empty list, never errors. Mirrors the
        // splice contract `,@()` contributes nothing.
        let params = MacroParams {
            required: vec!["a".into()],
            rest: Some("c".into()),
        };
        let vals = params.bind("m", &[Sexp::int(1)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(1), Sexp::List(vec![])]);
    }

    #[test]
    fn bind_missing_required_errors_before_any_rest_collection() {
        // A required name with no arg at its position is a
        // `MissingMacroArg` — the gate fires during the required walk,
        // before the rest is ever collected.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            rest: Some("c".into()),
        };
        let err = params
            .bind("m", &[Sexp::int(1)])
            .expect_err("missing required `b` must error");
        assert!(
            matches!(err, LispError::MissingMacroArg { .. }),
            "expected MissingMacroArg, got: {err:?}"
        );
    }

    #[test]
    fn bind_rest_less_params_ignore_surplus_args() {
        // Parity with the prior `Vec<Param>` binder: a rest-less param list
        // binds only its required slots; surplus call args are dropped, not
        // an error.
        let params = MacroParams {
            required: vec!["a".into()],
            rest: None,
        };
        let vals = params.bind("m", &[Sexp::int(1), Sexp::int(2)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(1)]);
    }
}
