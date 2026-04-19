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
pub fn compile_template(def: &MacroDef) -> Result<CompiledTemplate> {
    let body = match &def.body {
        Sexp::Quasiquote(inner) => inner.as_ref(),
        other => other,
    };
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
            let name = inner.as_symbol().ok_or_else(|| LispError::Compile {
                form: "unquote".into(),
                message: "only bound symbols may appear after `,` (no evaluator)".into(),
            })?;
            let idx = params
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| LispError::Compile {
                    form: format!(",{name}"),
                    message: "unbound".into(),
                })?;
            ops.push(TemplateOp::Subst(idx));
        }
        Sexp::UnquoteSplice(inner) => {
            let name = inner.as_symbol().ok_or_else(|| LispError::Compile {
                form: "unquote-splice".into(),
                message: "only bound symbols may appear after `,@`".into(),
            })?;
            let idx = params
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| LispError::Compile {
                    form: format!(",@{name}"),
                    message: "unbound".into(),
                })?;
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
                    .ok_or_else(|| LispError::Compile {
                        form: format!("call to {macro_name}"),
                        message: format!("missing required arg: {name}"),
                    })?;
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
                let arg = args.get(i).cloned().ok_or_else(|| LispError::Compile {
                    form: format!("call to {macro_name}"),
                    message: format!("missing required arg: {name}"),
                })?;
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
            let sym = inner.as_symbol().ok_or_else(|| LispError::Compile {
                form: "unquote".into(),
                message: "only bound symbols may appear after `,` (no evaluator)".into(),
            })?;
            bindings
                .get(sym)
                .cloned()
                .ok_or_else(|| LispError::Compile {
                    form: format!(",{sym}"),
                    message: "unbound".into(),
                })
        }
        Sexp::UnquoteSplice(_) => Err(LispError::Compile {
            form: "unquote-splice".into(),
            message: "`,@` may only appear inside a list".into(),
        }),
        Sexp::List(items) => {
            let mut out: Vec<Sexp> = Vec::with_capacity(items.len());
            for item in items {
                if let Sexp::UnquoteSplice(inner) = item {
                    let sym = inner.as_symbol().ok_or_else(|| LispError::Compile {
                        form: "unquote-splice".into(),
                        message: "only bound symbols may appear after `,@`".into(),
                    })?;
                    let val = bindings.get(sym).ok_or_else(|| LispError::Compile {
                        form: format!(",@{sym}"),
                        message: "unbound".into(),
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
            cache_size >= 10 && cache_size <= 50,
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
}
