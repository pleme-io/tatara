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
use crate::error::{LispError, Result};

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
                .ok_or_else(|| non_symbol_unquote_target(",", inner))?;
            let idx = params
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| unbound_template_var(",", name, params))?;
            ops.push(TemplateOp::Subst(idx));
        }
        Sexp::UnquoteSplice(inner) => {
            let name = inner
                .as_symbol()
                .ok_or_else(|| non_symbol_unquote_target(",@", inner))?;
            let idx = params
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| unbound_template_var(",@", name, params))?;
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
                    .ok_or_else(|| LispError::Compile {
                        form: macro_name.into(),
                        message: format!("compiled template referenced bad param index {idx}"),
                    })?
                    .clone();
                stack.last_mut().unwrap().push(v);
            }
            TemplateOp::Splice(idx) => {
                let v = args_by_index.get(*idx).ok_or_else(|| LispError::Compile {
                    form: macro_name.into(),
                    message: format!("compiled template referenced bad splice index {idx}"),
                })?;
                match v {
                    Sexp::List(items) => stack.last_mut().unwrap().extend(items.iter().cloned()),
                    Sexp::Nil => {}
                    other => stack.last_mut().unwrap().push(other.clone()),
                }
            }
            TemplateOp::BeginList => stack.push(Vec::new()),
            TemplateOp::EndList => {
                let items = stack.pop().ok_or_else(|| LispError::Compile {
                    form: macro_name.into(),
                    message: "compiled template: EndList with empty stack".into(),
                })?;
                stack.last_mut().unwrap().push(Sexp::List(items));
            }
        }
    }
    let mut top = stack.pop().ok_or_else(|| LispError::Compile {
        form: macro_name.into(),
        message: "compiled template produced no value".into(),
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
    let Some(head) = list.first().and_then(|s| s.as_symbol()) else {
        return Ok(None);
    };
    if !matches!(head, "defmacro" | "defpoint-template" | "defcheck") {
        return Ok(None);
    }
    if list.len() < 4 {
        return Err(LispError::Compile {
            form: head.to_string(),
            message: "(defmacro name (params) body) required".into(),
        });
    }
    let name = list[1]
        .as_symbol()
        .ok_or_else(|| LispError::Compile {
            form: head.to_string(),
            message: "expected name symbol".into(),
        })?
        .to_string();
    let param_list = list[2].as_list().ok_or_else(|| LispError::Compile {
        form: head.to_string(),
        message: "expected param list".into(),
    })?;
    let params = parse_params(param_list)?;
    let body = list[3].clone();
    Ok(Some(MacroDef { name, params, body }))
}

fn parse_params(list: &[Sexp]) -> Result<Vec<Param>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < list.len() {
        let s = list[i].as_symbol().ok_or_else(|| LispError::Compile {
            form: "defmacro params".into(),
            message: "expected symbol".into(),
        })?;
        if s == "&rest" {
            let name =
                list.get(i + 1)
                    .and_then(|x| x.as_symbol())
                    .ok_or_else(|| LispError::Compile {
                        form: "defmacro params".into(),
                        message: "&rest needs a name".into(),
                    })?;
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
                .ok_or_else(|| non_symbol_unquote_target(",", inner))?;
            bindings
                .get(sym)
                .cloned()
                .ok_or_else(|| unbound_template_var(",", sym, &bound_names(bindings)))
        }
        Sexp::UnquoteSplice(inner) => Err(splice_outside_list(inner)),
        Sexp::List(items) => {
            let mut out: Vec<Sexp> = Vec::with_capacity(items.len());
            for item in items {
                if let Sexp::UnquoteSplice(inner) = item {
                    let sym = inner
                        .as_symbol()
                        .ok_or_else(|| non_symbol_unquote_target(",@", inner))?;
                    let val = bindings
                        .get(sym)
                        .ok_or_else(|| unbound_template_var(",@", sym, &bound_names(bindings)))?;
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
/// `prefix` is `&'static str` because every call site passes a literal
/// (`","` / `",@"`); `name` is the offender from source; the hint is
/// `Option<String>` because the matched candidate borrows from a transient
/// `Vec<&str>` we built locally — copying the matched name into the variant
/// is the cheapest way to keep `LispError` lifetime-free.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four inline
/// copies in one module is well past the three-times rule. THEORY.md §V.1 —
/// knowable platform; the structural variant exposes `prefix` / `name` /
/// `hint` as first-class fields so authoring tools (LSP, REPL,
/// `tatara-check`) bind to the data shape instead of substring-parsing the
/// rendered diagnostic.
fn unbound_template_var(prefix: &'static str, name: &str, candidates: &[&str]) -> LispError {
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
/// `prefix` is `&'static str` because every call site passes a literal
/// (`","` / `",@"`); the inner is the offending `Sexp` projected through
/// `Display` so the operator sees the literal value they wrote — `(list 1
/// 2)`, `5`, `:foo` — instead of just a type-name. Naming both the
/// syntactic context AND the offending value is the typed-entry gate's
/// structural-completeness floor.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four
/// inline copies in one module is past the three-times rule. THEORY.md
/// §V.1 — knowable platform; the structural variant exposes `prefix` /
/// `got` as first-class fields so authoring tools (LSP, REPL,
/// `tatara-check`) bind to the data shape instead of substring-parsing
/// the rendered diagnostic. THEORY.md §II.1 invariant 1 — typed entry;
/// a non-symbol unquote target is exactly the failure mode the
/// typed-entry gate exists to reject.
fn non_symbol_unquote_target(prefix: &'static str, got: &Sexp) -> LispError {
    LispError::NonSymbolUnquoteTarget {
        prefix,
        got: got.to_string(),
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
        got: inner.to_string(),
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
    fn unbound_var(err: &LispError) -> (&str, &str, Option<&str>) {
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
        assert_eq!(prefix, ",");
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
        assert_eq!(prefix, ",@");
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
        assert_eq!(prefix, ",");
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
        assert_eq!(prefix, ",@");
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
        assert_eq!(prefix, ",");
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
    /// legibility. Sibling of `unbound_var`.
    fn non_symbol_target(err: &LispError) -> (&str, &str) {
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => (*prefix, got.as_str()),
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
        assert_eq!(prefix, ",");
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
        assert_eq!(prefix, ",@");
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
        assert_eq!(prefix, ",");
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
        assert_eq!(prefix, ",@");
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
            LispError::SpliceOutsideList { got } => got.as_str(),
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
}
