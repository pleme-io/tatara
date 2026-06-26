//! `TataraDomain` — a Rust type authorable as a Lisp `(<keyword> :k v …)` form.
//!
//! Apply `#[derive(TataraDomain)]` (from `tatara-lisp-derive`) and a plain
//! struct gains a full Lisp compiler: keyword dispatch, kwarg parsing, typed
//! field extraction.
//!
//! Also exposes a `DomainRegistry` + `linkme`-free `register_domain!` macro
//! so any crate that derives `TataraDomain` can auto-register itself; the
//! dispatcher then looks up unknown top-level forms by keyword at runtime.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::de::DeserializeOwned;

use crate::ast::Sexp;
use crate::error::{ExpectedKwargShape, KwargPath, LispError, Result, SexpShape, SexpWitness};

/// A Rust type compilable from a Lisp form.
pub trait TataraDomain: Sized {
    /// The Lisp keyword (e.g., `"defmonitor"`).
    const KEYWORD: &'static str;

    /// Parse the argument list (everything after the keyword) into Self.
    fn compile_from_args(args: &[Sexp]) -> Result<Self>;

    /// Parse a complete form; validates the head symbol matches `KEYWORD`.
    fn compile_from_sexp(form: &Sexp) -> Result<Self> {
        let list = form
            .as_list()
            .ok_or_else(|| not_a_list_form_err(Self::KEYWORD))?;
        // The two sub-modes of "head can't be projected to a symbol" — empty
        // list (`first()` is `None`) vs. present-but-not-a-symbol
        // (`as_symbol()` is `None`) — share ONE structural variant
        // (`MissingHeadSymbol { keyword, got }`) but bind to distinct
        // `got` payloads (`None` vs. `Some(<sexp display>)`). This lets
        // an authoring tool render "your form is empty" vs. "your
        // form's head is `5`, not a symbol" without re-parsing the
        // source — the legacy `Compile`-shaped diagnostic collapsed
        // both into one message.
        let head_sexp = list
            .first()
            .ok_or_else(|| missing_head_err(Self::KEYWORD, None))?;
        let head = head_sexp
            .as_symbol()
            .ok_or_else(|| missing_head_err(Self::KEYWORD, Some(sexp_witness(head_sexp))))?;
        if head != Self::KEYWORD {
            return Err(head_mismatch(Self::KEYWORD, head.to_string()));
        }
        Self::compile_from_args(&list[1..])
    }
}

// ── compile_from_sexp diagnostics — the form-shape gate primitives ─
//
// `compile_from_sexp` (the trait default) gates every `TataraDomain`
// invocation that takes a complete `(KEYWORD …)` form: ProcessSpec,
// MonitorSpec, AlertPolicySpec, every hand-written impl. Three failure
// modes — not a list, missing head symbol, wrong head — used to be
// inline `LispError::Compile { form: KEYWORD.to_string(), message: …}`
// triples in the trait default. The three-times-rule signal
// (THEORY.md §VI.1) calls for one named primitive per shape; these
// are them.
//
// All three are now structural: `not_a_list_form_err` returns
// `LispError::NotAListForm`, `missing_head_err` returns
// `LispError::MissingHeadSymbol { keyword, got }` (`got: None` for
// empty list, `got: Some(<sexp display>)` for present-but-not-symbol),
// and `head_mismatch` returns `LispError::HeadMismatch`. Each carries
// its distinguishing data (the offending head's display projection,
// the keyword) as first-class variant fields so authoring tools
// pattern-match structurally instead of substring-grepping the
// rendered message. The entire `compile_from_sexp` rejection chain
// — bare-atom → empty/not-symbol head → wrong-keyword head — is
// closed: every distinct typed-entry rejection at the form-shape
// gate binds to ONE structural variant of `LispError`.

/// `T::compile_from_sexp` was passed something that isn't a list.
/// One named primitive every TataraDomain impl shares — returns the
/// dedicated `LispError::NotAListForm { keyword }` variant so
/// authoring surfaces (REPL, LSP, `tatara-check`) bind to the
/// first-class `keyword` field instead of substring-parsing the
/// rendered message. Display matches the legacy `Compile`-shaped
/// diagnostic byte-for-byte (`"compile error in {keyword}: expected
/// list form"`), so existing `format!("{err}").contains("expected
/// list form")` assertions pass unchanged.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. The legacy
/// `Compile { form, message }` shape required consumers to
/// pattern-match on `message == "expected list form"` to recognize
/// this specific gate (versus the sibling `missing head symbol`
/// gate, which produces the same `Compile` shape with a different
/// message). After this lift the discriminator is the variant
/// itself — a regression that drifts the message string can no
/// longer drift the gate's identity. THEORY.md §II.1 invariant 1 —
/// typed entry; a non-list form is exactly the failure mode the
/// typed-entry gate exists to reject, and the gate's identity is
/// now load-bearing in the type system.
#[must_use]
pub fn not_a_list_form_err(keyword: &'static str) -> LispError {
    LispError::NotAListForm { keyword }
}

/// `T::compile_from_sexp` was passed `()` or a list whose first
/// element isn't a symbol — there's nothing to dispatch on. One named
/// primitive every `TataraDomain` impl shares; returns the dedicated
/// `LispError::MissingHeadSymbol { keyword, got }` variant so authoring
/// surfaces (REPL, LSP, `tatara-check`) bind to the first-class
/// `keyword` and `got` fields instead of substring-parsing the
/// rendered message. `got: None` for the empty-list case (`()`),
/// `got: Some(SexpWitness)` for the present-but-not-symbol case
/// (`(5 …)`, `(:foo …)`, `("x" …)`, `((nested) …)`) — the legacy
/// `Compile`-shaped diagnostic collapsed both into one message; this
/// builder bifurcates them structurally so the renderable detail
/// names which sub-mode fired. The `Some` arm carries the typed
/// joint identity (`SexpShape` + `Sexp::Display`) routed through
/// `sexp_witness(_)` so authoring tools that want to surface a
/// structural autofix — "you wrote `:foo` at the head slot where a
/// symbol was expected (did you mean `foo`?)" — bind on
/// `got.shape == SexpShape::Keyword` directly, no substring-grep on
/// the rendered display required.
///
/// Display matches the legacy `Compile`-shaped diagnostic byte-for-
/// byte for the prefix (`"compile error in {keyword}: missing head
/// symbol"`); the structural detail is appended in a parenthetical
/// (`(empty list)` for `None`, `(got {g})` for `Some(g)`), parallel
/// to how `RestParamMissingName` appends `(rest marker at position
/// {n}, {got|none provided})` and how `SpliceOutsideList` appends
/// `(got ,@{got})`. The `{g}` slot flows through `SexpWitness::Display`,
/// which writes only the `display` field, so existing
/// `format!("{err}").contains("missing head symbol")` assertions pass
/// unchanged.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. The legacy
/// `Compile { form, message }` shape required consumers to
/// pattern-match on `message == "missing head symbol"` to recognize
/// this specific gate (versus the sibling `expected list form` and
/// head-mismatch gates, which produced different `message` strings
/// in the same `Compile` shape). After this lift the discriminator
/// is the variant itself — a regression that drifts the message
/// string can no longer drift the gate's identity, AND the two
/// distinct sub-modes (empty vs. present-but-not-symbol) are
/// structurally addressable. THEORY.md §II.1 invariant 1 — typed
/// entry; an empty form / non-symbol-head form is exactly the
/// failure mode the typed-entry gate exists to reject, and the
/// gate's identity is now load-bearing in the type system.
#[must_use]
pub fn missing_head_err(keyword: &'static str, got: Option<SexpWitness>) -> LispError {
    LispError::MissingHeadSymbol { keyword, got }
}

/// Structural head-mismatch builder. Returns the dedicated
/// `LispError::HeadMismatch` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to first-class `keyword`/`got` fields instead
/// of substring-parsing the rendered message. Display matches the
/// legacy `Compile`-shaped diagnostic byte-for-byte, so existing
/// `format!("{err}").contains("expected ({KEYWORD}")` assertions pass
/// unchanged.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. A diagnostic
/// whose `got` is embedded in a free-form message is structurally
/// incomplete; an authoring surface that wants to render
/// "did-you-mean" suggestions on the offending head must re-parse
/// the message. After this lift the slot exists in the variant's
/// data shape itself.
#[must_use]
pub fn head_mismatch(keyword: &'static str, got: String) -> LispError {
    LispError::HeadMismatch { keyword, got }
}

// ── kwarg parsing + typed extractors used by the derive macro ──────

pub type Kwargs<'a> = HashMap<String, &'a Sexp>;

/// Parse `:k v :k v …` into a kwargs map. Rejects duplicate keywords so the
/// typed-entry gate fires on `(defX :name "a" :name "b")` instead of silently
/// keeping the last value — same posture `reject_unknown_kwargs` takes for
/// typo'd kwargs. A duplicate is ill-typed input: the author either meant
/// distinct keys (typo) or a list (`:tags ("a" "b")`).
///
/// Odd-length kwargs lists fail with `LispError::OddKwargs { dangling }`,
/// where `dangling` is the offending element's `Sexp::Display` projection
/// — `:query` for a keyword whose value got lost, or the literal form of a
/// stray non-keyword. Naming the dangling element keeps the diagnostic
/// structurally complete instead of merely flagging "odd number"; authoring
/// surfaces (REPL, LSP, `tatara-check`) render the mismatch without
/// re-reading the source.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — "Typed entry. Ill-typed input
/// errors before the value exists." THEORY.md §V.1 — "knowable platform"
/// requires the diagnostic to name what was passed, not only what was
/// expected.
pub fn parse_kwargs(args: &[Sexp]) -> Result<Kwargs<'_>> {
    let mut kw = HashMap::new();
    let mut i = 0;
    while i + 1 < args.len() {
        let key = args[i].as_keyword().ok_or_else(|| {
            type_mismatch(kwargs_pos_form(i), ExpectedKwargShape::Keyword, &args[i])
        })?;
        if kw.insert(key.to_string(), &args[i + 1]).is_some() {
            return Err(duplicate_kwarg(key));
        }
        i += 2;
    }
    if i < args.len() {
        return Err(LispError::OddKwargs {
            dangling: args[i].to_string(),
        });
    }
    Ok(kw)
}

/// Reject any keyword in `kw` that isn't in `allowed`. Closes the typed-entry
/// hole where typos like `:tthreshold 0.99` would otherwise parse silently
/// with the field unset. Emitted by `#[derive(TataraDomain)]` after
/// `parse_kwargs` so every derived domain rejects unknown kwargs by default.
///
/// When the offending keyword is a near-miss of an allowed kwarg (bounded
/// edit distance via `suggest`), the diagnostic prepends a `did you mean
/// :X?` hint so the operator goes straight to the fix without scanning the
/// allowed-list. The hint is purely additive — `unknown keyword` and the
/// full allowed list still appear — so existing assertions
/// (`msg.contains("unknown keyword")`, `msg.contains(":threshold")`) pass
/// unchanged.
///
/// Returns the structural `LispError::UnknownKwarg { key, hint, allowed }`
/// variant — same posture as the `OddKwargs` / `DuplicateKwarg` /
/// `MissingKwarg` siblings. After this lift every distinct typed-entry
/// kwarg-gate failure mode binds to ONE structural variant of `LispError`,
/// not a `Compile`-shaped substring.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — "Ill-typed input
/// errors before the value exists"); §V.1 ("knowable platform … Render
/// Anywhere" — naming the likely intended keyword is the floor of a
/// constructive diagnostic).
pub fn reject_unknown_kwargs(kw: &Kwargs<'_>, allowed: &[&str]) -> Result<()> {
    for key in kw.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(unknown_kwarg(key, allowed));
        }
    }
    Ok(())
}

/// Parse `:k v :k v …` AND gate the result against a closed allowed-key set —
/// the fused typed-entry kwargs gate. ONE named primitive every
/// `TataraDomain` impl shares for "compile-from-args header": every
/// `#[derive(TataraDomain)]`-generated `compile_from_args` body emitted by
/// `tatara-lisp-derive` begins with this single call, and every hand-
/// written impl in the forge / lattice / tameshi crates that wants the
/// substrate's closed-set kwargs posture binds to ONE function instead of
/// remembering to call [`parse_kwargs`] AND [`reject_unknown_kwargs`] in
/// that order.
///
/// Before this lift the derive emitted the two-call sequence
/// `let kw = parse_kwargs(args)?; reject_unknown_kwargs(&kw, ALLOWED)?;`
/// verbatim at every consumer's `compile_from_args` body — well past the
/// ≥2 PRIME-DIRECTIVE trigger once the fleet's seven-plus
/// `#[derive(TataraDomain)]` consumers (ProcessSpec, EphemeralSpec,
/// MonitorSpec, NotifySpec, AlertPolicySpec, EscalationStep, CompilerSpec,
/// and every future derived domain) inline the same two lines through the
/// proc-macro emitter. The two-call sequence is structurally one
/// operation — "parse the keyword/value run, then assert every key sits
/// in the static allowed-set" — and a regression that drifts ONE
/// consumer's gate from the others (e.g. the derive emits one call but a
/// hand-written impl emits only the other, or a future emitter swaps the
/// order so `reject_unknown_kwargs` runs against an unparsed slice) is
/// the silent typed-entry hole this primitive closes by construction.
///
/// The two stages are composed in the canonical order:
///   1. [`parse_kwargs`] runs first — odd-length input, non-keyword at a
///      key position, and duplicate keys surface as their structural
///      variants ([`LispError::OddKwargs`] / [`LispError::TypeMismatch`]
///      with `form = kwargs_pos_form(i)` / [`LispError::DuplicateKwarg`]).
///   2. Only on `Ok(kw)` does [`reject_unknown_kwargs`] run — keys
///      outside `allowed` surface as [`LispError::UnknownKwarg`] with the
///      typed `hint` / `allowed` slots populated.
///
/// This ordering is structural: `reject_unknown_kwargs` cannot inspect
/// an unparsed `&[Sexp]`, so parse-stage rejection MUST precede
/// reject-stage rejection. A call with BOTH an odd-length tail AND an
/// unknown kwarg surfaces as `OddKwargs` (parse-stage), never as
/// `UnknownKwarg` (reject-stage) — the gate is single-pass and the
/// stages compose in exactly one order. Naming the composition makes
/// that order load-bearing data on the substrate, not a discipline the
/// derive's emit template happens to encode correctly.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — "Typed entry. Ill-typed
/// input errors before the value exists." The kwargs gate is the
/// typed-entry boundary for every derived domain; closing the gate
/// behind ONE primitive lifts the closed-set posture from the derive's
/// emit template to the substrate's typed surface. THEORY.md §VI.1 —
/// generation over composition; the two-call sequence in the derive's
/// emit template, multiplied across every consumer in the fleet, is
/// well past the three-times rule once the structural shape is named.
/// THEORY.md §V.1 — knowable platform; authoring tools (REPL, LSP,
/// `tatara-check`) that want to surface "this form's kwargs gate
/// rejected because …" bind to the unified primitive's call site
/// instead of guessing which of the two component functions the
/// rejection came from. THEORY.md §II.1 invariant 2 (free middle) —
/// every consumer routes through the SAME composition, so a regression
/// that drifts the order or skips a stage on one path can never reach
/// the substrate's runtime: the type system binds every consumer to
/// the fused primitive's single emission shape.
///
/// Lifetime: the returned [`Kwargs<'a>`] borrows from `args` (the typed
/// alias is `HashMap<String, &'a Sexp>`), so the call site keeps the
/// `&[Sexp]` slice alive for the lifetime of the parsed map — same
/// posture as [`parse_kwargs`]. The fused primitive does not allocate
/// beyond [`parse_kwargs`]'s map: [`reject_unknown_kwargs`] is a pure
/// `O(allowed.len() · kw.len())` scan that returns `Ok(())` on success.
pub fn parse_kwargs_strict<'a>(args: &'a [Sexp], allowed: &[&str]) -> Result<Kwargs<'a>> {
    let kw = parse_kwargs(args)?;
    reject_unknown_kwargs(&kw, allowed)?;
    Ok(kw)
}

/// Structural unknown-kwarg builder. Returns the dedicated
/// `LispError::UnknownKwarg` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to first-class `key` / `hint` / `allowed`
/// fields instead of substring-parsing the rendered message. Display
/// matches the legacy `Compile { form: kwarg_form(key), message:
/// "unknown keyword (...)" }` rendering byte-for-byte
/// (`"compile error in :{key}: unknown keyword (did you mean :{hint}?;
/// allowed: :a, :b, :c)"` with a hint, `"compile error in :{key}:
/// unknown keyword (allowed: :a, :b, :c)"` without), so existing
/// `msg.contains("unknown keyword")` / `msg.contains(":threshold")` /
/// `msg.contains("did you mean :threshold?")` assertions keep
/// passing.
///
/// Encapsulates the three otherwise-inline steps every unknown-kwarg
/// site shares: (1) ranking the near-miss via `suggest`, (2) sorting
/// the allowed-set lexicographically so two operators on two machines
/// see the same message for the same input — diagnostics are
/// deterministic, (3) materializing the allowed-set as owned
/// `Vec<String>` so the variant lives independent of the call frame
/// and crosses thread boundaries cleanly. A future "registry-aware
/// near-miss for unknown registry-dispatched forms" path
/// (`tatara-check`'s unknown-keyword fallthrough) binds to this
/// helper rather than re-formatting the shape per call site.
///
/// `reject_unknown_kwargs` is the first consumer; hand-written
/// `TataraDomain` impls in the forge / lattice / tameshi crates that
/// don't fit the derive's closed-field-type set bind to the
/// substrate's primitive instead of inline `LispError::Compile { … }`
/// assembly. After this lift `reject_unknown_kwargs` is no longer the
/// last `LispError::Compile { ... }` site in the kwarg-gate's
/// diagnostic surface — every distinct kwarg-gate failure mode is now
/// a structural variant of `LispError`.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic whose offending `key` / hint / allowed-set
/// are embedded in a free-form message is structurally incomplete; an
/// authoring surface that wants to render a squiggly under the typo
/// or surface the allowed-set as completions must re-parse the
/// message. After this lift the slots exist in the variant's data
/// shape itself. THEORY.md §II.1 invariant 1 (typed entry) — an
/// unknown kwarg is exactly the failure mode the typed-entry gate
/// exists to reject; naming it structurally is the typed posture for
/// that gate's diagnostic. THEORY.md §VI.1 (generation over
/// composition — one named primitive per structural shape).
#[must_use]
pub fn unknown_kwarg(key: &str, allowed: &[&str]) -> LispError {
    let hint = suggest(key, allowed).map(String::from);
    let mut sorted: Vec<String> = allowed.iter().map(|s| (*s).to_string()).collect();
    sorted.sort();
    LispError::UnknownKwarg {
        key: key.to_string(),
        hint,
        allowed: sorted,
    }
}

/// The typed-entry kwargs-gate's OPTIONAL lookup primitive — `Some(&Sexp)`
/// when `key` is present in `kw`, `None` when absent. ONE named projection
/// on the substrate's `Kwargs<'a>` algebra every optional-kwarg consumer
/// (`extract_optional_atom`, `extract_list`, `extract_optional_via_serde`)
/// routes through, and the sibling [`required`](self::required) composes
/// directly atop it as `optional(kw, key).ok_or_else(|| missing_kwarg(key))`.
/// Before this lift the same `kw.get(key).copied()` projection — turning
/// `Option<&&'a Sexp>` (the raw `HashMap::get` return) into the consumer-
/// shaped `Option<&'a Sexp>` — was inlined verbatim at FOUR sites: once
/// inside `required`'s composition, and once inside each of the three
/// optional consumers' absence-handling preludes. After this lift the
/// projection lives in ONE place; `required` becomes the closed-form
/// composition `optional + ok_or_else(missing_kwarg)`, and the three
/// optional consumers read through `optional(kw, key)` without re-stating
/// the `Option<&&Sexp>` → `Option<&Sexp>` projection at each call site.
///
/// Sibling pair with [`required`](self::required): together the two close
/// the substrate's typed-entry kwargs-LOOKUP surface — `required` is the
/// mandatory-presence path returning `Result<&Sexp>` (absence → typed
/// `LispError::MissingKwarg`); `optional` is the may-be-absent path
/// returning `Option<&Sexp>` (absence → `None`, the consumer decides
/// what default behavior absence triggers — `None` for atoms, empty `Vec`
/// for lists, `Sexp::Nil` for params). The TWO primitives between them
/// cover every consumer's kwargs-lookup posture; a third would be a
/// structural extension the type system would surface at every call site.
/// The composition `required = optional + ok_or_else(missing_kwarg)` is
/// the structural identity binding the two — `required(kw, key)` and
/// `optional(kw, key).ok_or_else(|| missing_kwarg(key))` are
/// observationally identical, and naming the composition makes the
/// identity a substrate-owned theorem rather than a hand-inlined
/// duplication discipline four sites had to keep in lockstep.
///
/// The returned `&'a Sexp` carries the SAME lifetime contract as
/// [`required`](self::required)'s `Ok(&'a Sexp)` — the projection borrows
/// from the kwargs map's value slot via `.copied()`, so the optional
/// consumers can hold the reference through their absence-arm match
/// without an intermediate clone. `'a` is the outer borrow lifetime
/// (mirroring `required`); the inner `'_` is free so call sites with
/// `Kwargs<'a>` (the typical `parse_kwargs` output binding) and
/// `Kwargs<'static>` (a future static-bound shape) both type-check
/// uniformly.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four
/// inline copies of one structural projection past the three-times rule
/// once the structural shape is named. THEORY.md §V.1 — knowable
/// platform; the substrate's typed-entry kwargs-lookup surface is now
/// the named PAIR `{required, optional}` — authoring tools (REPL, LSP,
/// `tatara-check`) that want to surface "this domain reads kwarg X as
/// optional" bind to the `optional` primitive's signature, not the
/// HashMap-level `get` chain. THEORY.md §II.1 invariant 1 — typed entry;
/// the kwargs-lookup gate's two postures (required vs. optional) are
/// now structurally named, so a future fourth posture (e.g. "required
/// with non-empty constraint") extends the pair as a peer rather than
/// silently piggybacking on the inlined `get(key).copied()` chain.
/// THEORY.md §II.1 invariant 2 — free middle; the typed-entry kwargs
/// gate's lookup shape is uniform across every derived domain (and
/// every hand-written `TataraDomain` impl), so a future emitter that
/// wants to instrument the lookup (a span-aware lookup, a debug-mode
/// lookup logger) wraps ONE function rather than four inline sites.
#[must_use]
pub fn optional<'a>(kw: &'a Kwargs<'_>, key: &str) -> Option<&'a Sexp> {
    kw.get(key).copied()
}

/// The typed-entry kwargs-gate's REQUIRED lookup primitive — `Ok(&Sexp)`
/// when `key` is present in `kw`, `Err(LispError::MissingKwarg)` when
/// absent. Composes [`optional`](self::optional) (the may-be-absent
/// lookup) with [`missing_kwarg`](self::missing_kwarg) (the canonical
/// rejection on absence) so the substrate's typed-entry kwargs-lookup
/// surface is named as the PAIR `{required, optional}` with `required`
/// expressed as the closed-form composition of its two sibling
/// primitives. Sibling pair documented in [`optional`](self::optional).
pub fn required<'a>(kw: &'a Kwargs<'_>, key: &str) -> Result<&'a Sexp> {
    optional(kw, key).ok_or_else(|| missing_kwarg(key))
}

/// Canonical typed `form:` value for a kwarg-level `LispError::TypeMismatch`.
/// Every typed-entry diagnostic that names a kwarg (`required`, `type_err`,
/// `deserialize_err`, the duplicate-keyword paths in `parse_kwargs` and
/// `sexp_to_json`, the unknown-keyword path in `reject_unknown_kwargs`,
/// the non-list path in `extract_vec_via_serde`) routes through this one
/// helper, so authoring surfaces (REPL, LSP, `tatara-check`) bind to a
/// single named primitive rather than seven inline `format!(":{key}")`
/// copies.
///
/// Returns the typed `crate::error::KwargPath::Named(key.to_string())` value
/// directly — consumers feed it into `LispError::TypeMismatch.form: KwargPath`
/// where it is structurally bound via pattern-match (`KwargPath::Named(_)`),
/// not substring-matched. The canonical `:<key>` literal lives in ONE place
/// (`KwargPath`'s Display match arm) alongside its sibling shapes
/// `kwarg_item_form` / `kwargs_pos_form`, so a typo in any of the three
/// can never drift independent of the others.
///
/// Theory anchor: THEORY.md §VI.1 — "Generation over composition.
/// Three-times rule: when a pattern repeats three times, extract an
/// archetype/backend/synthesizer and generate from it." Seven inline
/// copies in one module is the textbook signal. THEORY.md §V.1 —
/// knowable platform; the typed `KwargPath` enum encodes the closed set
/// of three reachable path shapes at the type level so authoring tools
/// bind to path-shape identity rather than substring-matching the
/// rendered prefix. THEORY.md §II.1 invariant 1 (typed entry) — the
/// kwargs-path identity is now load-bearing data on the variant rather
/// than a projection-to-String.
#[must_use]
pub fn kwarg_form(key: &str) -> crate::error::KwargPath {
    crate::error::KwargPath::named(key)
}

/// Canonical `form:` label for a failure inside the Nth item of a
/// list-typed kwarg — `:steps[1]` when the second item of `:steps` fails
/// to deserialize, `:tags[2]` when the third tag isn't a string. The
/// substrate names the item-path so the operator sees both *which kwarg*
/// and *which element* misfired without re-counting from the source.
///
/// Frontier inspiration: JSON Pointer (`/steps/1`) and jq path
/// expressions — lossless paths through value projections so downstream
/// tooling (LSP underlines, structural rewrites) bind to the path
/// instead of parsing the diagnostic message. Translation through
/// pleme-io primitives: the surface syntax authors already write
/// (`:<key>` + `[idx]`), no new error variant, no new IR layer. When a
/// future run gives `Sexp` source spans, the indexed form gains a
/// position the same way `kwarg_form` will — one helper, every consumer
/// inherits.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic that names the kwarg but loses the item index
/// is structurally incomplete; the path completes it.
///
/// Returns the typed `crate::error::KwargPath::Item { key, idx }` value
/// directly — consumers feed it into `LispError::TypeMismatch.form: KwargPath`
/// where it is structurally bound via pattern-match (`KwargPath::Item { .. }`),
/// not substring-matched. The canonical `:<key>[<idx>]` literal lives in ONE
/// place alongside `kwarg_form` / `kwargs_pos_form`. See `kwarg_form` for the
/// typed-enum's role.
#[must_use]
pub fn kwarg_item_form(key: &str, idx: usize) -> crate::error::KwargPath {
    crate::error::KwargPath::item(key, idx)
}

/// Canonical `form:` label for a kwargs-list slot whose key position is
/// not yet known — the slot itself failed the
/// "this-position-must-be-a-keyword" gate, so there is no `:<key>` to
/// hang the path off. Renders `kwargs[<idx>]` — parallel to
/// `kwarg_item_form`'s `:<key>[<idx>]` shape, rooted at the kwargs
/// slice rather than at a named kwarg.
///
/// Used by `parse_kwargs` to label the structural type-mismatch when
/// the element at an even position isn't a `Sexp::Atom(Keyword(_))`.
/// Pairing this label with the existing `LispError::TypeMismatch`
/// variant (`expected: "keyword"`, `got: sexp_type_name(_)`) means
/// authoring surfaces (REPL, LSP, `tatara-check`) bind to ONE variant
/// identity for every typed-entry mismatch — `:<key>` for kwarg-level
/// failures, `:<key>[<idx>]` for per-item failures, and now
/// `kwargs[<idx>]` for not-a-keyword-yet failures. When a future run
/// gives `Sexp` source spans, the slot-form gains a position the same
/// way `kwarg_form` / `kwarg_item_form` will — one helper, every
/// consumer inherits.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// fourth `form:`-label primitive after `kwarg_form`,
/// `kwarg_item_form`, and the registry-keyword path; one helper per
/// distinct path shape so the substrate's diagnostic surface stays
/// structurally complete).
///
/// Returns the typed `crate::error::KwargPath::Slot(idx)` value directly —
/// consumers feed it into `LispError::TypeMismatch.form: KwargPath` where it
/// is structurally bound via pattern-match (`KwargPath::Slot(_)`), not
/// substring-matched. The canonical `kwargs[<idx>]` literal lives in ONE
/// place alongside `kwarg_form` / `kwarg_item_form`. See `kwarg_form` for
/// the typed-enum's role.
#[must_use]
pub fn kwargs_pos_form(idx: usize) -> crate::error::KwargPath {
    crate::error::KwargPath::Slot(idx)
}

/// Typed projection of a `Sexp`'s outermost shape into the closed-set
/// `SexpShape` enum — the twelve reachable shapes the reader can produce.
/// Used by the typed extractors to thread the observed shape into
/// `LispError::TypeMismatch.got: SexpShape` /
/// `LispError::NamedFormNonSymbolName.got: SexpShape` so a typed-entry
/// gate's rejection-shape identity is load-bearing data in the type
/// system, not a `&'static str` projection at the helper boundary.
/// Consumers (REPL, LSP, `tatara-check`) pattern-match on
/// `SexpShape::Int` etc. directly rather than substring-matching the
/// rendered `got` literal.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. An error that names
/// only the expected side leaves the operator to guess what was passed;
/// naming both is the floor of constructive diagnostics. The typed
/// projection extends that posture: not just naming both sides, but
/// encoding the observed shape's identity as a TYPE so a regression that
/// drifts the label becomes a compile error, not a runtime substring
/// drift. When a future run gives `Sexp` source spans, this helper is
/// the single site that learns to thread `got Y at <pos>`; today's call
/// sites pick up the span automatically.
#[must_use]
pub fn sexp_shape(s: &Sexp) -> SexpShape {
    match s {
        Sexp::Nil => SexpShape::Nil,
        // The six atomic-payload variants share the
        // `Sexp::Atom(_) → SexpShape::*` shape — all route through
        // `Atom::kind`'s typed closed-set projection so the per-variant
        // (Atom variant, SexpShape variant) pairing binds at ONE site on
        // the closed-set `AtomKind` algebra (`AtomKind::sexp_shape`)
        // rather than six byte-identical inline arms here. Sibling
        // posture to the quote-family collapse below (and to
        // `Hash for Atom`'s six-arm `hash_discriminator` collapse on the
        // atomic axis, and `Hash for Sexp`'s four-arm `hash_discriminator`
        // collapse on the quote-family axis). A future seventh atomic
        // kind (e.g. `Atom::Char` for `#\x` reader syntax) extends
        // `AtomKind` AND `Atom::kind` together, with rustc binding the
        // extension through the `AtomKind::sexp_shape` arm here — adding
        // it at one of the three sites without the other two becomes a
        // compile error, not a silent drift.
        Sexp::Atom(a) => a.kind().sexp_shape(),
        Sexp::List(_) => SexpShape::List,
        // The four quote-family variants share the
        // `Sexp::* → SexpShape::*` shape — all route through
        // `as_quote_form`'s typed-marker projection so the per-variant
        // (Sexp variant, SexpShape variant) pairing binds at ONE site on
        // the closed-set `QuoteForm` algebra (`QuoteForm::sexp_shape`)
        // rather than four byte-identical inline arms here. The
        // `.expect(_)` is a static-invariant statement (the outer pattern
        // guarantees the projection lands `Some`) — a future quote-family
        // extension that drifts `Sexp` AND `QuoteForm` apart fails at
        // rustc, not at runtime. Sibling posture to `Hash for Sexp`'s
        // four-arm `hash_discriminator` collapse, `Display for Sexp`'s
        // `prefix` collapse, and `interop`'s `iac_forge_tag` collapse.
        Sexp::Quote(_) | Sexp::Quasiquote(_) | Sexp::Unquote(_) | Sexp::UnquoteSplice(_) => {
            let (qf, _) = s.expect_quote_form();
            qf.sexp_shape()
        }
    }
}

/// Stable, human-readable name of a `Sexp`'s outermost shape — the
/// `&'static str` projection of `sexp_shape(_).label()`. Retained for
/// callers that want the canonical literal directly (e.g. test
/// assertions on the rendered `expected X, got Y` substring); new code
/// constructing `LispError::TypeMismatch` / `NamedFormNonSymbolName`
/// passes through `sexp_shape` directly so the typed identity rides
/// the variant slot rather than collapsing through the literal at the
/// helper boundary.
#[must_use]
pub fn sexp_type_name(s: &Sexp) -> &'static str {
    sexp_shape(s).label()
}

/// Typed projection of a `Sexp` into a `SexpWitness` — the joint
/// identity (structural `SexpShape` + renderable `Sexp::Display`
/// projection) the offending-value side of a typed-entry rejection
/// owes the operator. Pairs `sexp_shape(_)` with `Sexp::to_string()`
/// in ONE projection so every error-builder helper that previously
/// projected `&Sexp → String` via `to_string()` at the variant
/// boundary — discarding the `SexpShape` — now lifts to ONE primitive
/// that carries both halves of the identity through the variant slot
/// directly.
///
/// Sibling of `sexp_shape(&Sexp) -> SexpShape` (the shape-only
/// projection feeding `TypeMismatch.got` / `NamedFormNonSymbolName.got`)
/// and `sexp_type_name(&Sexp) -> &'static str` (the `&'static str`-only
/// projection feeding legacy substring-grep consumers). `sexp_witness`
/// is the typed JOINT projection — both halves of the identity bundled
/// into ONE owned `SexpWitness` value so the variant lives independent
/// of the call frame and crosses thread boundaries cleanly.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / constructive
/// diagnostics. An error that names only the shape leaves the operator
/// to guess what they wrote; an error that names only the literal
/// withholds the structural identity tools want to pattern-match on.
/// The witness names both. THEORY.md §VI.1 — generation over
/// composition; the seven `got: <sexp>.to_string()` projections at
/// error-builder boundaries (six in `macro_expand.rs`, one in
/// `domain.rs::missing_head_err`'s caller) collapse into ONE primitive
/// parameterized by `&Sexp`. THEORY.md §II.1 invariant 1 — typed
/// entry; the offending Sexp's identity is part of the proof of WHAT
/// the typed-entry gate rejected.
#[must_use]
pub fn sexp_witness(s: &Sexp) -> SexpWitness {
    SexpWitness::new(sexp_shape(s), s.to_string())
}

/// Suggest the candidate closest to `needle` by Levenshtein distance,
/// when the closest candidate is within a bounded edit distance.
///
/// The bound scales with `needle`'s character length:
///   - len ≤ 3: bound 1 (single-character typo on a short identifier)
///   - len ≤ 7: bound 2 (insertion + transposition, two typos)
///   - len ≥ 8: bound 3 (longer identifiers absorb more drift)
///
/// Returns the closest candidate within the bound. Ties are broken
/// lexicographically so two operators on two machines see the same hint
/// for the same input — diagnostics are deterministic. An exact match in
/// `candidates` is excluded (the caller already has the keyword; the
/// suggestion exists for near-misses only). Empty `candidates` returns
/// `None`.
///
/// One named primitive lifts the substrate's understanding of "near-match
/// across a candidate set" out of any per-call-site implementation. The
/// unknown-kwarg diagnostic in `reject_unknown_kwargs` is the first
/// consumer; future consumers — `LispError::HeadMismatch`'s "did you
/// mean a registered domain?" hint, `tatara-check`'s registry-dispatch
/// suggestions, the LSP's completion-failure fallback — bind to one
/// helper rather than re-implementing edit distance.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render Anywhere."
/// Naming the likely intended candidate is the floor of a constructive
/// diagnostic. THEORY.md §VI.1 — generation over composition: every
/// near-match suggestion in the substrate routes through ONE primitive.
///
/// Frontier inspiration: rustc's `find_best_match_for_name`, Idris's
/// "did you mean …?" elaborator hint, Roslyn's `SymbolMatcher` — bounded
/// edit distance over a symbol table. Translation through pleme-io
/// primitives: a pure function over `&[&str]`, no new error variant, no
/// new IR layer, no new dep.
#[must_use]
pub fn suggest<'a>(needle: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let bound = suggestion_bound(needle);
    let mut best: Option<(usize, &'a str)> = None;
    for &candidate in candidates {
        if candidate == needle {
            continue;
        }
        let dist = levenshtein(needle, candidate);
        if dist > bound {
            continue;
        }
        match best {
            None => best = Some((dist, candidate)),
            Some((bd, bc)) if dist < bd || (dist == bd && candidate < bc) => {
                best = Some((dist, candidate));
            }
            _ => {}
        }
    }
    best.map(|(_, c)| c)
}

fn suggestion_bound(needle: &str) -> usize {
    let n = needle.chars().count();
    if n <= 3 {
        1
    } else if n <= 7 {
        2
    } else {
        3
    }
}

/// Classic two-row Levenshtein. Operates on `char`s so multibyte input
/// (e.g. a domain authored with non-ASCII identifiers) measures
/// character-distance, not byte-distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Structural duplicate-kwarg builder. Returns the dedicated
/// `LispError::DuplicateKwarg` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to a first-class `key` field instead of
/// substring-parsing the rendered message. Display matches the legacy
/// `Compile { form: kwarg_form(key), message: "duplicate keyword" }`
/// rendering byte-for-byte (`"compile error in :{key}: duplicate
/// keyword"`), so existing `msg.contains("duplicate keyword")` /
/// `msg.contains(":name")` assertions keep passing.
///
/// Two inline copies of the same triple — `parse_kwargs`'s top-level
/// duplicate-keyword path and `sexp_to_json`'s nested-kwargs duplicate-
/// keyword path — used to assemble this shape by hand. One named
/// primitive lifts both into the substrate's structural-variant surface,
/// so every `parse_kwargs` failure mode (`OddKwargs` for odd length,
/// `TypeMismatch` for not-a-keyword-at-position, `DuplicateKwarg` for
/// duplicate key) is now a structural variant of `LispError`, not a
/// `Compile`-shaped substring.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic whose offending `key` is embedded in a
/// free-form message is structurally incomplete; an authoring surface
/// that wants to render a squiggly under the duplicate or hint a fix
/// must re-parse the message. After this lift the slot exists in the
/// variant's data shape itself. THEORY.md §II.1 invariant 1 (typed
/// entry — "Ill-typed input errors before the value exists") — a
/// duplicate kwarg is exactly the failure mode the typed-entry gate
/// exists to reject; naming it structurally is the typed posture for
/// that gate's diagnostic.
#[must_use]
pub fn duplicate_kwarg(key: &str) -> LispError {
    LispError::DuplicateKwarg {
        key: key.to_string(),
    }
}

/// Structural missing-kwarg builder. Returns the dedicated
/// `LispError::MissingKwarg` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to a first-class `key` field instead of
/// substring-parsing the rendered message. Display matches the legacy
/// `Compile { form: kwarg_form(key), message: "required but not
/// provided" }` rendering byte-for-byte (`"compile error in :{key}:
/// required but not provided"`), so existing
/// `msg.contains("required")` / `msg.contains(":threshold")` assertions
/// keep passing.
///
/// `required` (the kwarg lookup helper that fronts every typed
/// extractor — `extract_string`, `extract_int`, `extract_float`,
/// `extract_bool`, `extract_via_serde`, plus every hand-written
/// `TataraDomain` impl in the forge / lattice / tameshi crates) used
/// to assemble this shape inline. One named primitive lifts that into
/// the substrate's structural-variant surface, so every kwarg-level
/// "required-but-absent" failure routes through ONE function instead
/// of re-formatting the shape per call site. After this lift every
/// distinct `parse_kwargs` + `required` typed-entry kwarg failure mode
/// (odd length, not-a-keyword-at-position, duplicate key, missing
/// required key) is now a structural variant of `LispError`, not a
/// `Compile`-shaped substring.
///
/// Sibling of the pre-existing `Missing(&'static str)` variant —
/// `MissingKwarg` covers the runtime-key path the kwargs extractors
/// share (every derive-generated extractor and every hand-written
/// `TataraDomain` impl); `Missing` stays for compile-time-known names.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic whose offending `key` is embedded in a
/// free-form message is structurally incomplete; an authoring surface
/// that wants to render a squiggly under the missing kwarg slot or
/// render a "did you mean :X?" hint must re-parse the message. After
/// this lift the slot exists in the variant's data shape itself.
/// THEORY.md §II.1 invariant 1 (typed entry — "Ill-typed input errors
/// before the value exists") — a missing required kwarg is exactly the
/// failure mode the typed-entry gate exists to reject; naming it
/// structurally is the typed posture for that gate's diagnostic.
#[must_use]
pub fn missing_kwarg(key: &str) -> LispError {
    LispError::MissingKwarg {
        key: key.to_string(),
    }
}

/// Structural type-mismatch builder. Pairs a typed `form: KwargPath`
/// (typically `kwarg_form(_)` / `kwarg_item_form(_, _)` /
/// `kwargs_pos_form(_)`) with the static `expected` label and the `got`
/// projection of the offending `Sexp` through `sexp_type_name`. Returns
/// the dedicated `LispError::TypeMismatch` variant so authoring surfaces
/// (REPL, LSP, `tatara-check`) bind to first-class `form`/`expected`/`got`
/// fields — pattern-matching on `KwargPath::Item { .. }` etc. directly —
/// instead of substring-parsing the rendered message.
///
/// Three inline `format!("expected {X}, got {}", sexp_type_name(_))`
/// copies in this module (`type_err`, `extract_string_list` per-item,
/// `extract_vec_via_serde` non-list) used to assemble the same shape by
/// hand; the three-times rule (THEORY.md §VI.1) calls for one named
/// primitive. This is it. Future runs that thread `pos: Option<usize>`
/// from `Sexp` spans add ONE field to the variant; every type-mismatch
/// site inherits positional rendering with no consumer changes.
#[must_use]
pub fn type_mismatch(
    form: crate::error::KwargPath,
    expected: ExpectedKwargShape,
    got: &Sexp,
) -> LispError {
    LispError::TypeMismatch {
        form,
        expected,
        got: sexp_shape(got),
    }
}

fn type_err(key: &str, expected: ExpectedKwargShape, got: &Sexp) -> LispError {
    type_mismatch(kwarg_form(key), expected, got)
}

/// Item-indexed sibling of `type_err` — pairs `kwarg_item_form` with
/// `type_mismatch` so a per-item failure inside a list-typed kwarg names
/// `KwargPath::Item { key, idx }` plus the structural `expected`/`got` shape.
/// Used by `extract_string_list`'s per-item path; future per-item type-mismatch
/// sites (e.g. typed enums-of-strings, typed numeric vecs) bind here
/// rather than re-inlining the shape.
fn type_err_at(key: &str, idx: usize, expected: ExpectedKwargShape, got: &Sexp) -> LispError {
    type_mismatch(kwarg_item_form(key, idx), expected, got)
}

/// Required atomic-kwarg extractor — fronts every typed-atom public
/// `extract_X` helper (`extract_string`, `extract_int`, `extract_float`,
/// `extract_bool`). The four byte-identical inline shapes —
///
/// ```ignore
/// let v = required(kw, key)?;
/// v.as_X().ok_or_else(|| type_err(key, "<X-name>", v))
/// ```
///
/// — collapse to ONE generic primitive parameterized by the projection
/// function `project: FnOnce(&'a Sexp) -> Option<T>` and the typed-name
/// label `expected: &'static str`. The four-times rule (THEORY.md §VI.1)
/// is decisively crossed; lifting it into ONE primitive means the next
/// change to the typed-atom failure-projection shape (e.g. threading
/// `pos: Option<usize>` once `Sexp` carries spans, attaching a structural
/// `source: SexpTypeMismatch` chain) lands as ONE signature change inside
/// `extract_atom`, and all four public extractors pick up the upgrade
/// mechanically — no per-extractor edit, no per-extractor test drift.
///
/// `T` is generic so the helper handles both owned (`i64`, `f64`, `bool`)
/// and borrowed (`&'a str`) projections uniformly — the lifetime
/// threading `&'a Sexp → Option<&'a str>` works because every
/// `Sexp::as_*` method is `for<'b> fn(&'b Self) -> Option<…&'b str…>`;
/// the helper inherits that lifetime quantification through
/// `FnOnce(&'a Sexp) -> Option<T>`. Calling `extract_atom(kw, key,
/// "string", Sexp::as_string)` infers `T = &'a str`; calling
/// `extract_atom(kw, key, "int", Sexp::as_int)` infers `T = i64`.
///
/// Sibling of `extract_optional_atom` for the optional kwarg path —
/// together the two close every distinct typed-atom kwarg extractor's
/// shape: required vs. optional, returning `Result<T>` vs.
/// `Result<Option<T>>` from the same underlying projection. Future
/// extension to additional atomic types (e.g. `Atom::Bytes` if/when
/// added) is ONE one-line public delegate plus ONE call site — no
/// new error-path duplication.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition;
/// three-times rule decisively crossed (four byte-identical
/// extract+project+type-err shapes across `extract_string`,
/// `extract_int`, `extract_float`, `extract_bool`). THEORY.md §V.1 —
/// knowable platform / constructive diagnostics: the typed-atom
/// kwarg-failure projection lives in ONE primitive so authoring
/// surfaces (`tatara-check`, REPL, LSP) pick up the diagnostic-shape
/// promotion mechanically once the variant is structurally extended.
/// THEORY.md §II.1 invariant 1 — typed entry; the typed-atom
/// extractor IS the rust-level typed-entry gate for primitive kwargs,
/// and naming its single shape lifts the gate from four-site
/// duplication to one rust function the substrate's diagnostic
/// promotions hang off of.
fn extract_atom<'a, T, F>(
    kw: &'a Kwargs<'a>,
    key: &str,
    expected: ExpectedKwargShape,
    project: F,
) -> Result<T>
where
    F: FnOnce(&'a Sexp) -> Option<T>,
{
    let v = required(kw, key)?;
    project(v).ok_or_else(|| type_err(key, expected, v))
}

/// Optional sibling of `extract_atom` — collapses the four byte-identical
/// inline shapes of `extract_optional_string`, `extract_optional_int`,
/// `extract_optional_float`, `extract_optional_bool`:
///
/// ```ignore
/// match kw.get(key) {
///     None => Ok(None),
///     Some(v) => v.as_X().map(Some).ok_or_else(|| type_err(key, "<X-name>", v)),
/// }
/// ```
///
/// into ONE generic primitive. Same `T`/`project`/`expected` shape as
/// `extract_atom`; the difference is the `kw.get(key)` short-circuit at
/// the `None` arm — an absent kwarg is not an error for optional
/// extractors, only a malformed-present one is. The `.copied()` on
/// `kw.get(key)` projects `Option<&&'a Sexp>` to `Option<&'a Sexp>` so
/// the `project` call gets the same `&'a Sexp` shape as the required
/// path — type-checks against the same projection functions
/// (`Sexp::as_string`, `Sexp::as_int`, etc.) without per-call casts.
///
/// Future structural promotion of the type-mismatch diagnostic lands at
/// ONE call site inside this helper — same property as `extract_atom`.
fn extract_optional_atom<'a, T, F>(
    kw: &'a Kwargs<'a>,
    key: &str,
    expected: ExpectedKwargShape,
    project: F,
) -> Result<Option<T>>
where
    F: FnOnce(&'a Sexp) -> Option<T>,
{
    match optional(kw, key) {
        None => Ok(None),
        Some(v) => project(v)
            .map(Some)
            .ok_or_else(|| type_err(key, expected, v)),
    }
}

/// List-typed kwarg extractor — fronts every public `extract_*` helper
/// that reads a kwarg as a `Sexp::List` and projects each element to an
/// owned `T`. The two byte-identical inline skeletons —
///
/// ```ignore
/// let Some(v) = kw.get(key).copied() else { return Ok(Vec::new()) };
/// let list = v.as_list().ok_or_else(|| type_err(key, <list-shape>, v))?;
/// list.iter().enumerate().map(<per-item>).collect()
/// ```
///
/// — `extract_string_list` (each item projected via `as_string`, per-item
/// failure via `type_err_at`) and `extract_vec_via_serde` (each item via
/// `from_value_with_path`, per-item failure carrying `KwargPath::item`) —
/// collapse to ONE generic primitive parameterized by the outer-shape
/// label `list_shape: ExpectedKwargShape` and the per-element projection
/// `item: FnMut(usize, &Sexp) -> Result<T>`. The skeleton owns the three
/// fixed decisions both extractors share: absent kwarg → `Ok(Vec::new())`
/// (an absent list kwarg is the empty list, never an error — same posture
/// `extract_optional_atom` takes for absent atoms); present-but-not-a-list
/// → `type_err(key, list_shape, v)` (the outer-shape gate, labeled by the
/// caller-supplied `list_shape` so `ListOfStrings` vs. `List` stays a
/// per-caller decision, not baked into the skeleton); and the
/// `iter().enumerate().map(item).collect()` per-element walk that threads
/// the element index into the projection so per-item diagnostics can name
/// `:<key>[<idx>]` without re-counting from the source.
///
/// This is the list-family sibling of `extract_atom` / `extract_optional_atom`
/// (the atom-family generic projection primitives). Together the three close
/// every distinct typed-kwarg extractor's outer skeleton: required atom,
/// optional atom, and list. The per-element projection is `FnMut(usize,
/// &Sexp) -> Result<T>` — generic over `T` so it handles both the owned-
/// `String` (`extract_string_list`) and `DeserializeOwned`-`T`
/// (`extract_vec_via_serde`) element shapes uniformly, and threading the
/// `usize` index lets the projection construct the item-keyed
/// `KwargPath::Item { key, idx }` / `type_err_at` path the per-item gate
/// reports through.
///
/// Future structural promotion of the outer not-a-list diagnostic, or a
/// move to a fallible-streaming collect that short-circuits on the first
/// bad element with its position, lands at ONE site inside this helper —
/// both public list extractors pick up the upgrade mechanically, same
/// property `extract_atom` gives the four atom extractors.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// list-typed extractor skeleton recurs at two sites (the PRIME-DIRECTIVE
/// ≥2 trigger) and is lifted to one owner, exactly as the atom skeleton was.
/// THEORY.md §V.1 — knowable platform; the list-kwarg outer gate + per-item
/// path live in ONE primitive so authoring surfaces (`tatara-check`, REPL,
/// LSP) pick up diagnostic-shape promotions once, not per-extractor.
/// THEORY.md §II.1 invariant 1 — typed entry; the list extractor IS the
/// rust-level typed-entry gate for list-shaped kwargs, and naming its single
/// skeleton lifts the gate from two-site duplication to one function the
/// substrate's diagnostic promotions hang off of.
fn extract_list<T, F>(
    kw: &Kwargs<'_>,
    key: &str,
    list_shape: ExpectedKwargShape,
    mut item: F,
) -> Result<Vec<T>>
where
    F: FnMut(usize, &Sexp) -> Result<T>,
{
    let Some(v) = optional(kw, key) else {
        return Ok(Vec::new());
    };
    let list = v.as_list().ok_or_else(|| type_err(key, list_shape, v))?;
    list.iter()
        .enumerate()
        .map(|(idx, e)| item(idx, e))
        .collect()
}

pub fn extract_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<&'a str> {
    extract_atom(kw, key, ExpectedKwargShape::String, Sexp::as_string)
}

pub fn extract_optional_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<Option<&'a str>> {
    extract_optional_atom(kw, key, ExpectedKwargShape::String, Sexp::as_string)
}

pub fn extract_string_list(kw: &Kwargs<'_>, key: &str) -> Result<Vec<String>> {
    extract_list(kw, key, ExpectedKwargShape::ListOfStrings, |idx, s| {
        s.as_string()
            .map(String::from)
            .ok_or_else(|| type_err_at(key, idx, ExpectedKwargShape::String, s))
    })
}

pub fn extract_int(kw: &Kwargs<'_>, key: &str) -> Result<i64> {
    extract_atom(kw, key, ExpectedKwargShape::Int, Sexp::as_int)
}

pub fn extract_optional_int(kw: &Kwargs<'_>, key: &str) -> Result<Option<i64>> {
    extract_optional_atom(kw, key, ExpectedKwargShape::Int, Sexp::as_int)
}

pub fn extract_float(kw: &Kwargs<'_>, key: &str) -> Result<f64> {
    extract_atom(kw, key, ExpectedKwargShape::Number, Sexp::as_float)
}

pub fn extract_optional_float(kw: &Kwargs<'_>, key: &str) -> Result<Option<f64>> {
    extract_optional_atom(kw, key, ExpectedKwargShape::Number, Sexp::as_float)
}

pub fn extract_bool(kw: &Kwargs<'_>, key: &str) -> Result<bool> {
    extract_atom(kw, key, ExpectedKwargShape::Bool, Sexp::as_bool)
}

pub fn extract_optional_bool(kw: &Kwargs<'_>, key: &str) -> Result<Option<bool>> {
    extract_optional_atom(kw, key, ExpectedKwargShape::Bool, Sexp::as_bool)
}

// ── Universal serde-Deserialize fallthrough (enums, nested structs, …) ──
//
// `#[derive(TataraDomain)]` covers `String` / numeric / `bool` / their
// `Option` and `Vec<String>` shapes with the typed extractors above. Any
// field type outside that closed set falls through to these helpers, which
// project the kwarg `Sexp` to canonical JSON via `sexp_to_json` and feed
// it to `serde_json::from_value` — works for any `serde::Deserialize`.
//
// The shape used to live inline in three `quote!` blocks in the derive
// macro (`Kind::Deserialize`, `Kind::OptionalDeserialize`,
// `Kind::VecDeserialize`). Lifting them here means:
//   - Hand-written `TataraDomain` impls share the same error path.
//   - Future diagnostic upgrades (attaching a source position once `Sexp`
//     carries spans, richer field-path traces) happen in ONE function,
//     not three macro-emitted copies.
//   - The `:<key> deserialize: …` message is a single named primitive in
//     the substrate — `tatara-check` / LSP / REPL render it uniformly.
//
// Both helpers below funnel through the structural
// `LispError::KwargDeserialize { path: KwargPath, message }` variant —
// the typed-entry-side `from_value` mirror of the typed-exit-side
// `to_value` `LispError::DomainSerialize { keyword, message }` lift. The
// two sites bifurcate via the typed `KwargPath` enum's variant identity:
// `KwargPath::Named(key)` for kwarg-keyed failures (the
// `extract_via_serde` / `extract_optional_via_serde` path),
// `KwargPath::Item { key, idx }` for kwarg-AND-index-keyed failures (the
// `extract_vec_via_serde` per-item path). After this lift the
// `from_value` boundary's two distinct rejection modes BOTH bind to ONE
// structural variant of `LispError`, not a `Compile`-shaped substring;
// the `(key, idx: Option<usize>)` bifurcation collapses into
// `KwargPath`'s `Named` vs. `Item` variant identity, so the invalid
// sibling-slot combination `(key: "", idx: Some(_))` for a scalar path
// is structurally unrepresentable rather than re-asserted at the helper
// boundary via runtime `Option::is_some` comparison. Together with
// `DomainSerialize`, every distinct `serde_json` failure mode at the
// typed-domain JSON boundary — both directions of the round-trip — is
// now structurally typed. This is the LAST `LispError::Compile { ... }`
// construction site in this file.
//
// Theory anchor: THEORY.md §VI.1 (generation over composition — the
// generator must lean on the library, not duplicate the library inline).
// THEORY.md §II.1 invariant 1 (typed entry) — `from_value` failures are
// exactly the failure mode the typed-entry JSON gate exists to reject;
// naming them structurally is the typed posture for that gate's
// diagnostic.

/// Project a single `&Sexp` through the typed-entry JSON boundary —
/// `sexp_to_json` canonical-JSON projection + `serde_json::from_value::<T>`
/// + structural `LispError::KwargDeserialize { path, message }` on failure.
///
/// THREE call sites in this module used to assemble this shape inline:
/// `extract_via_serde` (required scalar kwarg path), `extract_optional_via_serde`
/// (optional scalar kwarg path), and `extract_vec_via_serde`'s per-item
/// closure (each item in a `Vec<T>` kwarg). The three byte-identical
/// `let json = sexp_to_json(sexp)?; serde_json::from_value(json).map_err(|e|
/// deserialize_*_err(<path-args>, &e))` shapes — modulo the typed
/// `KwargPath` constructor (`KwargPath::Named` vs. `KwargPath::Item`) —
/// collapse to ONE primitive parameterized by `path: KwargPath`. The
/// path's variant identity bifurcates scalar-vs-item rendering inside
/// `KwargPath`'s Display impl (`:<key>` vs. `:<key>[<idx>]`) so the helper
/// is shape-of-typed-entry-JSON-boundary, not shape-of-call-site.
///
/// After this lift the three-times-rule on the `from_value` projection
/// shape is decisively crossed; the two prior-run thin `deserialize_err`
/// / `deserialize_item_err` shims — which encapsulated only the
/// `KwargPath::named(_)` / `KwargPath::item(_,_)` constructor projection
/// over an already-extant `serde_json::Error` reference — are subsumed
/// by this primitive's `map_err` closure. The three extractor entry
/// points now bind on `from_value_with_path::<T>` directly with their
/// `KwargPath` constructed at the call boundary; the JSON-boundary's
/// rejection shape (`LispError::KwargDeserialize { path, message }`)
/// lives in ONE place — the `map_err` arm here — instead of being
/// re-asserted at three site-specific shims.
///
/// `<T: DeserializeOwned>` is generic so the helper handles every serde-
/// projectable typed-domain field uniformly — scalar `i64` / `String` /
/// nested struct / `Vec<Nested>` / enum-by-symbol — same posture as the
/// `extract_atom` / `extract_optional_atom` generic-projection primitives
/// for the atom-typed kwarg path. `path: KwargPath` flows into the
/// variant's typed slot directly (owned), parallel to how `type_mismatch`
/// threads `KwargPath` into `LispError::TypeMismatch.form`. A future
/// fourth path shape (e.g. `:<key>.<field>` for nested-struct kwarg
/// failures) extends `KwargPath` ONCE and rustc-enforces matching at
/// every projection site; this helper picks up the new shape mechanically
/// with no signature change.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// three-times rule's load-bearing trigger. THEORY.md §V.1 — knowable
/// platform; the typed-entry JSON-projection boundary's rejection shape
/// lives in ONE primitive so authoring surfaces (`tatara-check`, REPL,
/// LSP) pick up the diagnostic-shape promotion mechanically once the
/// variant is structurally extended. THEORY.md §II.1 invariant 1 (typed
/// entry) — a `from_value` failure is exactly the failure mode the
/// typed-entry JSON gate exists to reject; naming its single shape lifts
/// the gate from three-site duplication to one rust function the
/// substrate's diagnostic promotions hang off of.
fn from_value_with_path<T: DeserializeOwned>(sexp: &Sexp, path: KwargPath) -> Result<T> {
    let json = sexp_to_json(sexp)?;
    serde_json::from_value(json).map_err(|e| LispError::KwargDeserialize {
        path,
        message: e.to_string(),
    })
}

/// Required field — feeds the kwarg's canonical-JSON projection to
/// `serde_json::from_value::<T>` via `from_value_with_path` with a
/// `KwargPath::Named(key)` path slot. Errors carry `:key` so authoring
/// tools can point at the offending kwarg.
pub fn extract_via_serde<T: DeserializeOwned>(kw: &Kwargs<'_>, key: &str) -> Result<T> {
    from_value_with_path(required(kw, key)?, KwargPath::named(key))
}

/// Optional field — `None` if the kwarg is absent; `Some(T)` after a
/// successful `from_value_with_path` round-trip with a `KwargPath::Named(key)`
/// path slot.
pub fn extract_optional_via_serde<T: DeserializeOwned>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<T>> {
    let Some(sexp) = optional(kw, key) else {
        return Ok(None);
    };
    from_value_with_path(sexp, KwargPath::named(key)).map(Some)
}

/// `Vec<T>` field — empty vec if the kwarg is absent; otherwise the kwarg
/// must be a `Sexp::List` and each item flows through `from_value_with_path`
/// with a `KwargPath::Item { key, idx }` path slot, naming both the outer
/// kwarg AND the failing item index in any per-item rejection.
pub fn extract_vec_via_serde<T: DeserializeOwned>(kw: &Kwargs<'_>, key: &str) -> Result<Vec<T>> {
    extract_list(kw, key, ExpectedKwargShape::List, |idx, item| {
        from_value_with_path(item, KwargPath::item(key, idx))
    })
}

// ── Domain registry (runtime-registered, callable by keyword) ───────

/// Erased handler that knows how to compile a form and hand back a typed
/// serde-JSON representation. JSON is the least-common-denominator typed
/// surface — every `TataraDomain` derives `serde::Serialize` by convention.
pub struct DomainHandler {
    pub keyword: &'static str,
    pub compile: fn(args: &[Sexp]) -> Result<serde_json::Value>,
}

static REGISTRY: OnceLock<Mutex<HashMap<&'static str, DomainHandler>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<&'static str, DomainHandler>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a `TataraDomain` type with the global dispatcher.
/// Idempotent — repeated registrations overwrite.
pub fn register<T>()
where
    T: TataraDomain + serde::Serialize,
{
    let handler = DomainHandler {
        keyword: T::KEYWORD,
        compile: |args| {
            let v = T::compile_from_args(args)?;
            serde_json::to_value(&v).map_err(serialize_to_json_err::<T>)
        },
    };
    registry().lock().unwrap().insert(T::KEYWORD, handler);
}

/// Look up a handler by keyword.
pub fn lookup(keyword: &str) -> Option<DomainHandler> {
    let reg = registry().lock().unwrap();
    reg.get(keyword).map(|h| DomainHandler {
        keyword: h.keyword,
        compile: h.compile,
    })
}

/// List currently registered keywords.
pub fn registered_keywords() -> Vec<&'static str> {
    registry().lock().unwrap().keys().copied().collect()
}

/// Suggest the registered domain keyword closest to `needle`, when the
/// closest one is within a bounded edit distance (`suggest`'s contract).
///
/// Wraps `suggest` over `registered_keywords()` so consumers don't repeat
/// the candidate-set assembly per call site. Authoring surfaces with an
/// unknown registry-dispatched form (`tatara-check`'s unknown-keyword
/// fallthrough, future LSP completion-failure paths, REPL hints) bind to
/// ONE primitive instead of pulling the keyword set themselves and
/// re-implementing edit-distance ranking. The result is `&'static str`
/// because every registered keyword is itself `'static` (the trait's
/// `KEYWORD` const), so the substrate hands back the exact same pointer
/// the registry stores — no allocation, no lifetime juggling.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic that says "unknown form: `defmoniter`" but
/// withholds the registered near-miss forces the operator to scan the
/// registry's keyword list visually; naming the candidate is the floor
/// of a constructive diagnostic. THEORY.md §VI.1 — generation over
/// composition: every "near-miss across the registry" lookup routes
/// through ONE primitive.
#[must_use]
pub fn suggest_keyword(needle: &str) -> Option<&'static str> {
    let keywords = registered_keywords();
    suggest(needle, &keywords)
}

/// Structural unknown-domain-keyword builder. Returns the dedicated
/// `LispError::UnknownDomainKeyword` variant so authoring surfaces
/// (`tatara-check`, the REPL, the LSP) bind to first-class
/// `keyword` / `hint` / `registered` fields instead of substring-parsing
/// the rendered message. The shape mirrors `unknown_kwarg`: the
/// kwarg-gate's unknown-allowed-set rejection and the registry-gate's
/// unknown-registered-set rejection share ONE structural primitive shape
/// across two structural variants — the substrate's diagnostic surface
/// stays uniform.
///
/// Encapsulates the three otherwise-inline steps every unknown-domain-
/// keyword site shares: (1) ranking the near-miss via `suggest_keyword`
/// over `registered_keywords()`, (2) sorting the registered set
/// lexicographically so two operators on two machines see the same
/// message for the same input — diagnostics are deterministic regardless
/// of HashMap iteration order, (3) materializing the registered set as
/// owned `Vec<String>` so the variant lives independent of the call frame
/// and crosses thread boundaries cleanly.
///
/// `tatara-check`'s registry-dispatch fallthrough is the first consumer;
/// hand-written authoring surfaces (LSP completion-failure fallback, REPL
/// hints, future multi-error collectors that name every unregistered
/// `(defX …)` form in one pass) bind to ONE function instead of
/// re-formatting the shape per call site.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render Anywhere."
/// A diagnostic whose offending `keyword` / `hint` / `registered`-set are
/// embedded in a free-form message is structurally incomplete; an
/// authoring surface that wants to render a squiggly under the typo or
/// surface the registered set as completions must re-parse the message.
/// After this lift the slots exist in the variant's data shape itself.
/// THEORY.md §VI.1 — generation over composition: every "near-miss across
/// the registry" lookup routes through `suggest_keyword`, every "diagnose
/// an unregistered head against the registry" routes through this
/// primitive.
#[must_use]
pub fn unknown_domain_keyword(keyword: &str) -> LispError {
    let hint = suggest_keyword(keyword).map(String::from);
    let mut registered: Vec<String> = registered_keywords()
        .into_iter()
        .map(String::from)
        .collect();
    registered.sort();
    LispError::UnknownDomainKeyword {
        keyword: keyword.to_string(),
        hint,
        registered,
    }
}

// ── Sexp ↔ serde_json bridge (universal type support) ──────────────
//
// Lets the derive macro fall through to `serde_json::from_value` for any
// field type implementing `Deserialize`. Handles enums (via symbol→string),
// nested structs (via kwargs→object), and `Vec<T>` of either.

use serde_json::Value as JValue;

/// Convert a Sexp to its canonical JSON form.
///
/// Rules:
///   - Symbols + Keywords → `Value::String`
///     (symbols are enum discriminants; keywords prefix with `:`)
///   - Strings, ints, floats, bools → their JSON counterpart
///   - Lists that look like `:k v :k v …` → `Value::Object`
///   - Other lists → `Value::Array`
///   - Quote/Quasiquote/Unquote/UnquoteSplice → convert the inner (strips quote)
///
/// Fails on a duplicate keyword inside any nested kwargs-list (e.g.
/// `(:notify-ref "a" :notify-ref "b")`) — same typed-entry posture
/// `parse_kwargs` takes at the top level. The round-trip path
/// (`json_to_sexp` → `sexp_to_json`) is unaffected because
/// `serde_json::Map` is unique-keyed by construction.
pub fn sexp_to_json(s: &Sexp) -> Result<JValue> {
    Ok(match s {
        Sexp::Nil => JValue::Null,
        // The six atomic-payload arms (one per `AtomKind` variant) all
        // routed through inline `Sexp::Atom(Atom::<variant>(payload))
        // => JValue::<…>(…)` pattern-binding pre-lift. Post-lift they
        // bind at ONE typed-algebra method on the closed-set `Atom`
        // algebra (`Atom::to_json`) that every consumer surfaces
        // through. Sibling-arm shape to the prior `Display for Atom`
        // lift (the canonical-string rendering surface) and the
        // upcoming `Atom::to_iac_forge_sexpr` (the canonical-SExpr
        // rendering surface, feature-gated `iac-forge`) — every per-
        // `Atom`-variant projection now binds at ONE method rather
        // than at six inline arms inside its consumer. A future
        // seventh atomic kind (e.g. `Char` for `#\x` reader syntax)
        // extends `AtomKind` + the typed-projection methods once and
        // this outer arm picks up the new variant for free through
        // [`Atom::to_json`]'s closed-set match.
        Sexp::Atom(a) => a.to_json(),
        Sexp::List(items) => {
            if is_kwargs_list(items) {
                let mut map = serde_json::Map::with_capacity(items.len() / 2);
                let mut i = 0;
                while i + 1 < items.len() {
                    if let Some(k) = items[i].as_keyword() {
                        let value = sexp_to_json(&items[i + 1])?;
                        if map.insert(kebab_to_camel(k), value).is_some() {
                            return Err(duplicate_kwarg(k));
                        }
                        i += 2;
                    } else {
                        break;
                    }
                }
                JValue::Object(map)
            } else {
                JValue::Array(items.iter().map(sexp_to_json).collect::<Result<Vec<_>>>()?)
            }
        }
        // The four quote-family variants share the `Sexp::* →
        // sexp_to_json(inner)?` recurse-on-inner shape — all route
        // through `Sexp::as_quote_form`'s typed-marker projection so
        // the per-variant (Sexp variant, inner: &Sexp) pairing binds
        // at ONE site on the `Sexp` algebra rather than four
        // byte-identical inline arms here. The marker is discarded
        // (`_`) by design: this projection erases the quote-form
        // identity into JSON, and the round-trip via `json_to_sexp`
        // re-emits the inner without an enclosing wrapper. Sibling
        // posture to `sexp_shape`'s arm at line 602 (the canonical
        // discard-the-marker, project-the-inner shape), and to every
        // OTHER quote-family consumer the substrate carries that
        // routes through `as_quote_form` / its derived `as_unquote`
        // 2-of-4 subset (`Hash for Sexp`'s `hash_discriminator`,
        // `Display for Sexp`'s `prefix`, `interop::iac_forge_tag`,
        // `contains_unquote`'s family check, `compile_template`'s
        // splice-outside-list gate, `compile_node`'s per-arm
        // bytecode emission, `substitute`'s top-level + list-inner
        // arms). A future homoiconic prefix-wrapper extension
        // (`QuoteForm::*Future*`) extends `as_quote_form`'s arm
        // alongside this match's discriminator union — exhaustively
        // checked at the algebra rather than re-derived per
        // consumer.
        Sexp::Quote(_) | Sexp::Quasiquote(_) | Sexp::Unquote(_) | Sexp::UnquoteSplice(_) => {
            let (_, inner) = s.expect_quote_form();
            sexp_to_json(inner)?
        }
    })
}

/// Convert serde_json back to Sexp — inverse of `sexp_to_json`.
/// Used by `rewrite_typed` to round-trip a typed value through Lisp forms.
pub fn json_to_sexp(v: &JValue) -> Sexp {
    match v {
        JValue::Null => Sexp::Nil,
        JValue::Bool(b) => Sexp::boolean(*b),
        JValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Sexp::int(i)
            } else if let Some(f) = n.as_f64() {
                Sexp::float(f)
            } else {
                Sexp::int(0)
            }
        }
        JValue::String(s) => Sexp::string(s.clone()),
        JValue::Array(items) => Sexp::List(items.iter().map(json_to_sexp).collect()),
        JValue::Object(map) => {
            let mut out = Vec::with_capacity(map.len() * 2);
            for (k, v) in map {
                out.push(Sexp::keyword(camel_to_kebab(k)));
                out.push(json_to_sexp(v));
            }
            Sexp::List(out)
        }
    }
}

fn is_kwargs_list(items: &[Sexp]) -> bool {
    !items.is_empty()
        && items.len().is_multiple_of(2)
        && items.iter().step_by(2).all(|s| s.as_keyword().is_some())
}

/// `must-reach` → `mustReach`, `point-type` → `pointType`.
fn kebab_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `mustReach` → `must-reach` (inverse of `kebab_to_camel`).
fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ── TypedRewriter — the self-optimization primitive ────────────────
//
// Takes a typed value, converts to Sexp, applies a Lisp rewrite, then
// re-enters the typed boundary via `compile_from_args`. Any rewrite that
// passes the typed re-validation is safe by construction — the Rust type
// system is the floor.

/// Promote the previously `LispError::Compile`-shaped helper into the
/// structural `LispError::DomainSerialize { keyword, message }` variant
/// — the typed-exit-side `to_value` mirror of the typed-entry-side
/// `NamedFormNonSymbolName` / typed-exit-side `RewriterNonList` lifts.
/// The gate fires when `serde_json::to_value` of a typed `T` value
/// errors at two byte-identical sites: `register::<T>`'s registry-
/// dispatch closure (serializes the just-typed value to JSON for the
/// dispatcher) and `rewrite_typed::<T>`'s round-trip prelude
/// (serializes the input to JSON before projecting to a `Sexp::List`
/// for the rewriter closure). Both share the exact same failure mode
/// and the exact same `keyword` projection from `T::KEYWORD`; the lift
/// promotes them to ONE structural variant so authoring tools (REPL,
/// LSP, `tatara-check`) bind on variant identity rather than
/// substring-grepping the rendered diagnostic.
///
/// `<T: TataraDomain>` is the type-level boundary: the `T::KEYWORD`
/// projection is mechanically applied to the variant's `keyword: &'static
/// str` slot, so a typo can never drift across the two call sites — the
/// type system is the floor, same posture as `RewriterNonList.keyword`,
/// `NamedFormMissingName.keyword`, `NamedFormNonSymbolName.keyword`,
/// `NotAListForm.keyword`, `MissingHeadSymbol.keyword`,
/// `HeadMismatch.keyword`, and the `Defmacro*.head` family. The helper
/// takes `serde_json::Error` by value so
/// `map_err(serialize_to_json_err::<T>)` composes point-free at every
/// site — no `.into()` boilerplate, no `&e` borrow at the call site.
/// The `serde_json::Error::Display` projection is materialized into the
/// variant's `message: String` slot at the boundary so the variant
/// lives independent of the call frame and the original error chain
/// (other variants in this enum are also `String`-carrying;
/// participating in the same Display contract keeps every consumer's
/// rendering pipeline uniform).
///
/// Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
/// — `"compile error in {keyword}: serialize: {message}"` — so
/// existing substring-grep consumers (`tatara-check`'s diagnostic
/// capture, REPL substring-greps that match on `"serialize: "`) pass
/// unchanged across the lift. The redundant-keyword `"serialize
/// {KEYWORD}: …"` shape that `rewrite_typed` carried pre-canonicalize
/// is already gone (the canonicalize step landed before the structural
/// lift); both sites render the cleaner `"serialize: …"` shape now.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// three-times rule, decisively crossed across two functions in this
/// file (`register::<T>` + `rewrite_typed::<T>`, two sites; the third
/// `to_value`-side gate `rewriter_non_list_err::<T>` immediately below
/// is the typed-exit-list sibling). After this lift the `to_value`
/// boundary's two distinct rejection modes BOTH bind to structural
/// variants of `LispError` keyed on `T::KEYWORD` —
/// `DomainSerialize { keyword, message }` (serialize-failed) +
/// `RewriterNonList { keyword, got }` (output-wrong-shape) — so the
/// substrate's typed-exit JSON surface is structurally complete on the
/// emission side. The `from_value` direction (the typed-entry JSON
/// boundary, kwargs-path-keyed via `deserialize_err` /
/// `deserialize_item_err`) now binds to the sibling
/// `LispError::KwargDeserialize { path: KwargPath, message }` variant,
/// closing the round-trip's last `LispError::Compile { ... }` site in
/// this file; both directions of the JSON-projection boundary are
/// structural. THEORY.md §II.1 invariant 1
/// (typed entry) + invariant 3 (typed exit): the JSON-projection
/// round-trip is the proof; the helper names its rejection shape at
/// the type level so authoring surfaces bind to a uniform "serialize-
/// failed" structural shape regardless of whether the failure
/// originated at registry-dispatch time or rewriter time.
fn serialize_to_json_err<T: TataraDomain>(e: serde_json::Error) -> LispError {
    LispError::DomainSerialize {
        keyword: T::KEYWORD,
        message: e.to_string(),
    }
}

/// Promote the previously `LispError::Compile`-shaped helper into the
/// structural `LispError::RewriterNonList { keyword, got }` variant —
/// the typed-exit-side mirror of the typed-entry-side
/// `NamedFormNonSymbolName` lift. The gate enforces the rewriter's
/// `Sexp::List` contract: the round-trip projects a typed value to
/// `Sexp::List` via `json_to_sexp`, hands that list to the rewriter
/// `F`, and re-enters `T::compile_from_args` via the list's items. A
/// non-list result violates the round-trip's structural promise — this
/// helper names that violation at the type level so authoring tools
/// (REPL, LSP, `tatara-check`) bind on variant identity rather than
/// substring-grepping the rendered diagnostic.
///
/// After this lift the self-optimization primitive's rejection chain is
/// structurally typed at BOTH boundaries: typed-entry
/// (`NamedFormMissingName`, `NamedFormNonSymbolName`) AND typed-exit
/// (this variant). The typed-entry chain rejects a wrong-shaped author-
/// supplied form before `compile_from_args` runs; the typed-exit chain
/// rejects a wrong-shaped rewriter output before `compile_from_args`
/// re-runs on the round-tripped representation. Every distinct rejection
/// mode in `rewrite_typed::<T>` is now a pattern-matchable variant.
///
/// `<T: TataraDomain>` carries the keyword projection — `T::KEYWORD`
/// (`&'static str`) flows into the variant's `keyword` slot at the
/// boundary so a typo in the keyword can never drift into the diagnostic
/// at runtime, same posture as `NamedFormNonSymbolName.keyword`,
/// `NamedFormMissingName.keyword`, `MissingHeadSymbol.keyword`,
/// `HeadMismatch.keyword`, `NotAListForm.keyword`, and the
/// `Defmacro*.head` family. The helper takes `got: &Sexp` and projects
/// it through `sexp_witness(got)` at the boundary so the variant's
/// `got: SexpWitness` slot carries the rewriter's offending output as
/// the typed joint identity — BOTH the structural `SexpShape` AND the
/// `Sexp::Display` literal in ONE owned value, parallel to the seven
/// typed-ENTRY-side lifts (`SpliceOutsideList.got: SexpWitness`,
/// `NonSymbolUnquoteTarget.got: SexpWitness`,
/// `NonSymbolParam.got: SexpWitness`,
/// `DefmacroNonSymbolName.got: SexpWitness`,
/// `DefmacroNonListParams.got: SexpWitness`,
/// `RestParamMissingName.got: Option<SexpWitness>`,
/// `MissingHeadSymbol.got: Option<SexpWitness>`). This is the EIGHTH
/// `SexpWitness` consumer and the FIRST on the typed-EXIT boundary —
/// before this lift the typed-exit-side `got` slot projected through
/// `Sexp::to_string()`, discarding the `SexpShape` identity at the
/// variant boundary the way the seven entry-side slots used to. The
/// value (not just its sexp-type) is the actionable diagnostic detail
/// for a typed-exit rejection — authoring a rewriter that returns the
/// wrong value is the failure mode being named — AND the structural
/// shape is now load-bearing alongside it.
///
/// Display preserves the legacy `"compile error in {keyword}: rewriter
/// must return a list; got {got}"` shape byte-for-byte so authoring
/// tools that pattern-matched on the pre-lift rendered string see no
/// drift across the lift; tools that pattern-match on the variant
/// gain structural binding to `keyword` AND `got` (BOTH the typed
/// shape via `got.shape` AND the literal via `got.display`). The
/// `{got}` slot flows through `SexpWitness::Display`, which writes
/// only the `display` field, so the rendering is byte-for-byte
/// identical to the pre-lift `got: String` shape.
///
/// Theory anchor: THEORY.md §II.1 invariant 3 (typed exit) —
/// `rewrite_typed` IS the typed-exit gate of the self-optimization
/// primitive; any rewrite that survives the gate is well-typed by
/// construction, AND now the rejection mode's offending-value identity
/// is itself structurally typed at the variant slot, the same posture
/// the seven typed-ENTRY-side lifts established for invariant 1.
/// THEORY.md §V.1 — knowable platform; the typed witness exposes BOTH
/// `got.shape` AND `got.display` as first-class fields so authoring
/// tools bind to the joint identity instead of substring-parsing the
/// rendered diagnostic to recover the shape. THEORY.md §VI.1 —
/// generation over composition; the one inline `got.to_string()`
/// projection at the helper boundary collapses into
/// `sexp_witness(got)` — the typed joint primitive — extending the
/// typed-identity unification contract from the seven entry-side
/// `Sexp::Display`-source `got` slots to the eighth (exit-side) slot.
/// After this lift EVERY `Sexp::Display`-source `got` slot in the
/// substrate, ENTRY-side OR EXIT-side, carries the SAME typed
/// `SexpWitness` primitive — the typed-identity unification contract
/// is closed across BOTH boundaries of the typed-IR algebra.
fn rewriter_non_list_err<T: TataraDomain>(got: &Sexp) -> LispError {
    LispError::RewriterNonList {
        keyword: T::KEYWORD,
        got: sexp_witness(got),
    }
}

/// Rewrite a typed `T` through Lisp form and re-validate on the way back.
///
/// The rewriter receives the value's kwargs representation (a `Sexp::List`
/// of alternating keywords + values) and returns a modified kwargs list.
/// `T::compile_from_args` validates the result — any ill-formed rewrite
/// produces a typed error; any well-formed rewrite produces a valid `T`.
pub fn rewrite_typed<T, F>(input: T, rewrite: F) -> Result<T>
where
    T: TataraDomain + serde::Serialize,
    F: FnOnce(Sexp) -> Result<Sexp>,
{
    let json = serde_json::to_value(&input).map_err(serialize_to_json_err::<T>)?;
    let sexp = json_to_sexp(&json);
    let rewritten = rewrite(sexp)?;
    let args = match rewritten {
        Sexp::List(items) => items,
        other => return Err(rewriter_non_list_err::<T>(&other)),
    };
    T::compile_from_args(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::read;
    use serde::{Deserialize, Serialize};
    use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

    /// Example domain authorable as Lisp — proves derive macro, trait, and
    /// registry all agree end-to-end.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defmonitor")]
    struct MonitorSpec {
        name: String,
        query: String,
        threshold: f64,
        window_seconds: Option<i64>,
        tags: Vec<String>,
        enabled: Option<bool>,
    }

    #[test]
    fn derive_emits_correct_keyword() {
        assert_eq!(MonitorSpec::KEYWORD, "defmonitor");
    }

    #[test]
    fn derive_compiles_full_form() {
        let forms = read(
            r#"(defmonitor
                 :name "prom-up"
                 :query "up{job='prometheus'}"
                 :threshold 0.99
                 :window-seconds 300
                 :tags ("prod" "observability")
                 :enabled #t)"#,
        )
        .unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(
            spec,
            MonitorSpec {
                name: "prom-up".into(),
                query: "up{job='prometheus'}".into(),
                threshold: 0.99,
                window_seconds: Some(300),
                tags: vec!["prod".into(), "observability".into()],
                enabled: Some(true),
            }
        );
    }

    #[test]
    fn derive_accepts_missing_optionals() {
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(spec.name, "x");
        assert!(spec.window_seconds.is_none());
        assert!(spec.enabled.is_none());
        assert!(spec.tags.is_empty());
    }

    #[test]
    fn derive_errors_on_missing_required() {
        let forms = read(r#"(defmonitor :name "x" :query "q")"#).unwrap();
        assert!(MonitorSpec::compile_from_sexp(&forms[0]).is_err());
    }

    #[test]
    fn derive_errors_on_wrong_head() {
        let forms = read(r#"(not-a-monitor :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(format!("{err}").contains("expected (defmonitor"));
    }

    #[test]
    fn derive_rejects_unknown_keyword() {
        // Typed-entry invariant (THEORY.md §II.1.1) — a typo'd keyword
        // must surface as an error before the value exists, not parse
        // silently with the field unset.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("tthreshold"),
            "error must name the offending keyword, got: {msg}"
        );
        assert!(
            msg.contains("unknown keyword"),
            "error must label the failure mode, got: {msg}"
        );
    }

    #[test]
    fn derive_unknown_keyword_lists_allowed_set() {
        // The error message includes the allowed-keyword set so the
        // operator can fix the typo without consulting the source.
        let forms = read(r#"(defmonitor :name "x" :ttreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":threshold"),
            "expected :threshold listed: {msg}"
        );
        assert!(msg.contains(":query"), "expected :query listed: {msg}");
        assert!(msg.contains(":name"), "expected :name listed: {msg}");
    }

    #[test]
    fn reject_unknown_kwargs_helper_passes_when_all_known() {
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        assert!(reject_unknown_kwargs(&kw, allowed).is_ok());
    }

    #[test]
    fn reject_unknown_kwargs_helper_errors_on_extra() {
        let forms = read(r#"(defmonitor :name "x" :ghost "boo")"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &["name"];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        assert!(format!("{err}").contains("ghost"));
    }

    #[test]
    fn registry_dispatches_by_keyword() {
        register::<MonitorSpec>();
        assert!(registered_keywords().contains(&"defmonitor"));
        let handler = lookup("defmonitor").expect("registered");
        assert_eq!(handler.keyword, "defmonitor");
        let forms = read(r#"(ignored :name "prom" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let json = (handler.compile)(&args[1..]).unwrap();
        assert_eq!(json["name"], "prom");
        assert_eq!(json["query"], "q");
        assert_eq!(json["threshold"], 0.5);
    }

    // ── extract_via_serde / extract_optional_via_serde / extract_vec_via_serde ──
    //
    // These helpers used to live as three inline `quote!` blocks in
    // tatara-lisp-derive. Pinning their behavior here means a hand-written
    // `TataraDomain` impl can rely on the same contract the derive uses,
    // and a regression that re-inlines the boilerplate fails-loudly here
    // before it fans out.

    #[derive(Deserialize, Debug, PartialEq)]
    enum Severity {
        Info,
        Warning,
        Critical,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct EscalationStep {
        notify_ref: String,
        wait_minutes: Option<i64>,
    }

    fn kwargs_of(src: &str) -> Vec<Sexp> {
        // `(_ :k v :k v …)` — strip the head, return the kwargs slice.
        let forms = read(src).unwrap();
        let list = forms[0].as_list().unwrap();
        list[1..].to_vec()
    }

    #[test]
    fn extract_via_serde_parses_enum_from_symbol() {
        // `:level Critical` — bare symbol → enum discriminant via the
        // sexp_to_json bridge → serde Deserialize.
        let args = kwargs_of("(_ :level Critical)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Severity = extract_via_serde(&kw, "level").unwrap();
        assert_eq!(s, Severity::Critical);
    }

    #[test]
    fn extract_via_serde_parses_nested_struct_from_kwargs_list() {
        let args = kwargs_of(r#"(_ :step (:notify-ref "oncall" :wait-minutes 5))"#);
        let kw = parse_kwargs(&args).unwrap();
        let s: EscalationStep = extract_via_serde(&kw, "step").unwrap();
        assert_eq!(
            s,
            EscalationStep {
                notify_ref: "oncall".into(),
                wait_minutes: Some(5),
            }
        );
    }

    #[test]
    fn extract_via_serde_missing_required_errors() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        let msg = format!("{err}");
        // The `required` helper supplies the missing-kwarg message — same
        // path the typed extractors use, so authoring tools render
        // missing kwargs uniformly across both fallthroughs.
        assert!(
            msg.contains(":level"),
            "missing-kwarg error must name the kwarg, got: {msg}"
        );
        assert!(
            msg.contains("required"),
            "expected 'required' in missing-kwarg error, got: {msg}"
        );
    }

    #[test]
    fn extract_via_serde_deserialize_failure_labels_keyword() {
        // `:level NotASeverity` — well-formed Sexp, ill-formed enum.
        // The error must point at `:level` so the operator can fix the
        // typo without inspecting the source twice. Bind on the
        // structural `LispError::KwargDeserialize { key, idx: None,
        // message }` variant — pinning the variant identity AND `idx:
        // None` (no item index for the scalar path) makes the
        // typed-entry `from_value` rejection mode load-bearing in the
        // type system; the legacy `Compile`-shaped substring-match on
        // `":level"` / `"deserialize:"` is preserved as a separate
        // assertion below for substring-grep consumers.
        let args = kwargs_of("(_ :level NotASeverity)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(ref key),
                    ref message,
                } if key == "level" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Named(\"level\"), .. }}, got {err:?}"
        );
        let msg = format!("{err}");
        assert!(
            msg.contains(":level"),
            "deserialize error must name the kwarg, got: {msg}"
        );
        assert!(
            msg.contains("deserialize:"),
            "expected 'deserialize:' label, got: {msg}"
        );
    }

    #[test]
    fn extract_optional_via_serde_returns_none_when_absent() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Option<Severity> = extract_optional_via_serde(&kw, "level").unwrap();
        assert!(s.is_none());
    }

    #[test]
    fn extract_optional_via_serde_returns_some_when_present() {
        let args = kwargs_of("(_ :level Warning)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Option<Severity> = extract_optional_via_serde(&kw, "level").unwrap();
        assert_eq!(s, Some(Severity::Warning));
    }

    #[test]
    fn extract_vec_via_serde_returns_empty_when_absent() {
        // Absent-kwarg → empty `Vec` — same semantics `Vec<String>` gets
        // through `extract_string_list`. Authoring surfaces can rely on
        // "no entry == empty list" without a `#[serde(default)]` dance.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let v: Vec<EscalationStep> = extract_vec_via_serde(&kw, "steps").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn extract_vec_via_serde_collects_nested_structs() {
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "a" :wait-minutes 0)
                  (:notify-ref "b" :wait-minutes 5)
                  (:notify-ref "c")))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let v: Vec<EscalationStep> = extract_vec_via_serde(&kw, "steps").unwrap();
        assert_eq!(
            v,
            vec![
                EscalationStep {
                    notify_ref: "a".into(),
                    wait_minutes: Some(0),
                },
                EscalationStep {
                    notify_ref: "b".into(),
                    wait_minutes: Some(5),
                },
                EscalationStep {
                    notify_ref: "c".into(),
                    wait_minutes: None,
                },
            ]
        );
    }

    #[test]
    fn extract_vec_via_serde_rejects_non_list_kwarg() {
        // `:steps "scalar"` — a list-typed kwarg given a scalar must fail
        // with the kwarg name in the form, so the operator sees what to
        // change.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(msg.contains("expected list"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_item_failure_labels_keyword() {
        // First item is well-formed; second item has a typo'd field.
        // The error must still point at `:steps`, even though the
        // failure is inside an item. Bind on the structural
        // `LispError::KwargDeserialize { key, idx: Some(1), message }`
        // variant — pinning `idx: Some(1)` (the failing item index)
        // makes the per-item rejection path structurally distinct from
        // the scalar / `Option<T>` path (`idx: None`); the legacy
        // substring-match on `":steps"` / `"deserialize:"` is preserved
        // as a separate assertion below for substring-grep consumers.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "ok")
                  (:notify-ref 7)))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { ref key, idx: 1 },
                    ref message,
                } if key == "steps" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Item {{ key: \"steps\", idx: 1 }}, .. }}, got {err:?}"
        );
        let msg = format!("{err}");
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(msg.contains("deserialize:"), "got: {msg}");
    }

    // ── Duplicate-keyword rejection (typed-entry hardening) ─────────────
    //
    // A typo like `:name "x" :name "y"` used to silently overwrite — the
    // last value wins, the operator gets no signal. Same bug class
    // `reject_unknown_kwargs` (commit 2750f39) closed for typo'd kwargs;
    // this closes the dual hole for duplicate kwargs at every nesting
    // level (top-level args, nested struct kwargs, vec item kwargs).
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry —
    // "Ill-typed input errors before the value exists").

    #[test]
    fn parse_kwargs_rejects_duplicate_top_level_keyword() {
        let args = kwargs_of(r#"(_ :name "x" :name "y")"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":name"),
            "error must name the keyword, got: {msg}"
        );
        assert!(
            msg.contains("duplicate keyword"),
            "expected 'duplicate keyword' label, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_accepts_distinct_keywords() {
        // Negative-control: pre-existing flow is preserved.
        let args = kwargs_of(r#"(_ :name "x" :query "q" :threshold 0.5)"#);
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(kw.len(), 3);
    }

    #[test]
    fn extract_via_serde_rejects_duplicate_in_nested_struct() {
        // `:step (:notify-ref "a" :notify-ref "b")` — the duplicate fires
        // during the `sexp_to_json` projection, before serde sees a value.
        let args = kwargs_of(r#"(_ :step (:notify-ref "a" :notify-ref "b"))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<EscalationStep>(&kw, "step").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":notify-ref"),
            "duplicate-in-nested error must name the inner kwarg, got: {msg}"
        );
        assert!(
            msg.contains("duplicate keyword"),
            "expected 'duplicate keyword' label, got: {msg}"
        );
    }

    #[test]
    fn extract_vec_via_serde_rejects_duplicate_in_item() {
        // `:steps ((:notify-ref "a" :notify-ref "b"))` — the duplicate is
        // inside one vec item. Authors get the same diagnostic shape
        // whether the duplicate is at the top level, in a nested struct,
        // or inside a vec item.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":notify-ref"), "got: {msg}");
        assert!(msg.contains("duplicate keyword"), "got: {msg}");
    }

    #[test]
    fn derive_rejects_duplicate_top_level_kwarg() {
        // End-to-end through `#[derive(TataraDomain)]` — silent overwrite
        // is exactly the bug class the typed-entry gate exists to prevent,
        // and every derived domain inherits the rejection by sharing
        // `parse_kwargs`.
        let forms = read(r#"(defmonitor :name "x" :name "y" :query "q" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":name"), "got: {msg}");
        assert!(msg.contains("duplicate"), "got: {msg}");
    }

    #[test]
    fn json_to_sexp_round_trip_does_not_trip_duplicate_check() {
        // The round-trip path used by `rewrite_typed`: a typed value
        // → `serde_json::Value` (unique-keyed) → `Sexp` via `json_to_sexp`
        // → top-level kwargs slice → `parse_kwargs`. The duplicate-check
        // gate must NOT false-positive on this canonical input.
        let original = MonitorSpec {
            name: "x".into(),
            query: "q".into(),
            threshold: 0.5,
            window_seconds: None,
            tags: vec![],
            enabled: None,
        };
        let json = serde_json::to_value(&original).unwrap();
        let sexp = json_to_sexp(&json);
        let args = sexp.as_list().expect("object → kwargs list").to_vec();
        let _kw = parse_kwargs(&args).expect("round-trip kwargs are unique by construction");
    }

    #[test]
    fn sexp_to_json_round_trip_array_unaffected_by_duplicate_check() {
        // Arrays-of-objects round-trip: each object is unique-keyed by
        // virtue of being authored as a `serde_json::Map`. The strict
        // duplicate check must not false-positive on this shape.
        let json = serde_json::json!([
            { "notifyRef": "a", "waitMinutes": 0 },
            { "notifyRef": "b", "waitMinutes": 5 },
        ]);
        let sexp = json_to_sexp(&json);
        let back = sexp_to_json(&sexp).expect("round-trip array must not trip duplicate check");
        // The array is preserved (object key order is stable inside each
        // element because `json_to_sexp` writes kwargs in iteration order
        // and `sexp_to_json` reads them back in the same order).
        assert_eq!(back, json);
    }

    #[test]
    fn sexp_to_json_routes_quote_family_arms_through_as_quote_form_typed_marker() {
        // PATH-UNIFORMITY CONTRACT: the lifted `sexp_to_json` routes
        // its four quote-family arms through `Sexp::as_quote_form()`,
        // discarding the marker and recursing on the inner — the
        // same typed-marker dispatch `sexp_shape` lifts at line 602.
        // Pin the new boundary three ways across `QuoteForm::ALL` so
        // a regression that drifts ONE variant's recurse-on-inner
        // shape from the others (e.g. an arm that mis-routes the
        // marker as the recursion subject, or drops the recursion
        // entirely returning the inner verbatim with the wrapper
        // collapsed to JSON null) fails-loudly here:
        //
        //   (1) sweep `QuoteForm::ALL`, wrapping a non-trivial
        //       kwargs inner (`(:name "payload")`) — the inner
        //       MUST project to the same JSON object regardless of
        //       which quote-family wrapper sits at the outer node.
        //       Catches a regression that mis-routes ONE variant
        //       (e.g. `Sexp::Quote(_)` arm returns `JValue::Null`
        //       instead of recursing) without the others.
        //   (2) sweep `QuoteForm::ALL`, asserting `as_quote_form`'s
        //       typed-marker projection AGREES with the constructor
        //       branch — proves the lifted arm and the algebra
        //       projection share ONE pairing of (Sexp variant,
        //       QuoteForm variant) and a regression that drifts the
        //       pairing surfaces here, not in production.
        //   (3) sweep `QuoteForm::ALL`, asserting
        //       `sexp_to_json(wrap_qf(inner)) ==
        //       sexp_to_json(as_quote_form(wrap_qf(inner)).inner)`
        //       — proves the lifted recursion target IS the
        //       `as_quote_form`-projected inner (not a clone, not a
        //       stale closure binding, not the outer node itself).
        //
        // Sibling posture to `sexp_shape`'s path-uniformity test at
        // line 2203 (the canonical reference shape for this lift).
        use crate::ast::QuoteForm;
        let inner = Sexp::List(vec![Sexp::keyword("name"), Sexp::string("payload")]);
        let expected = sexp_to_json(&inner).expect("inner must serialize cleanly");

        for qf in QuoteForm::ALL {
            let wrapped = qf.wrap(inner.clone());
            let via_lifted =
                sexp_to_json(&wrapped).expect("quote-family wrapper must serialize cleanly");
            assert_eq!(
                via_lifted, expected,
                "sexp_to_json drifted from `sexp_to_json(inner)` at quote-family marker {qf:?}"
            );
            let (marker, projected_inner) = wrapped
                .as_quote_form()
                .expect("quote-family wrapper must project through as_quote_form");
            assert_eq!(
                marker, qf,
                "as_quote_form drifted the typed marker at {qf:?}"
            );
            let via_composed =
                sexp_to_json(projected_inner).expect("projected inner must serialize cleanly");
            assert_eq!(
                via_lifted, via_composed,
                "sexp_to_json drifted from as_quote_form + recurse(inner) at {qf:?}"
            );
        }
    }

    #[test]
    fn sexp_to_json_quote_family_arms_recurse_on_inner_not_outer() {
        // INTENT-PIN: pre-lift the four quote-family arms each
        // pattern-bound `inner` and recursed on `inner`, NEVER on the
        // outer wrapper. Post-lift the recursion target comes from
        // `as_quote_form`'s projection tuple. Pin that this binding
        // semantic is observable end-to-end: a `,@'inner-form` shape
        // — `UnquoteSplice` wrapping `Quote` wrapping a kwargs list
        // — MUST collapse through BOTH wrappers and project the
        // innermost kwargs list as the JSON object, NOT either
        // wrapper's JSON-Null projection. A regression that lifted
        // the recursion onto `s` (the outer wrapper) instead of the
        // `as_quote_form`-projected inner would infinite-loop here
        // (or, with the stack-overflow guard, produce a stack
        // overflow); a regression that collapsed the wrappers
        // independently would skip the inner recursion and emit
        // partial JSON. The double-wrapper exercises BOTH the
        // outermost `as_quote_form` projection AND the recursive
        // step's projection — the same shape `compile_node`'s
        // bytecode emission exercises when a quasi-quote template
        // nests inside another quasi-quote.
        let inner_payload = Sexp::List(vec![Sexp::keyword("k"), Sexp::int(42)]);
        let expected = serde_json::json!({ "k": 42 });
        // ,@'(...) — UnquoteSplice wraps Quote wraps the kwargs list.
        let doubly_wrapped =
            Sexp::UnquoteSplice(Box::new(Sexp::Quote(Box::new(inner_payload.clone()))));
        let via_lifted =
            sexp_to_json(&doubly_wrapped).expect("double-wrapper must serialize cleanly");
        assert_eq!(
            via_lifted, expected,
            "sexp_to_json must recurse THROUGH every quote-family wrapper and \
             project the innermost shape — a regression that lifted recursion \
             onto the outer wrapper would diverge or emit JSON null here"
        );
    }

    #[test]
    fn sexp_to_json_atom_arms_route_through_atom_to_json() {
        // LIFTED-BOUNDARY CONTRACT: pin that the lifted `sexp_to_json`
        // routes its six atomic-payload arms through the typed-algebra
        // method [`crate::ast::Atom::to_json`]. Pre-lift the per-variant
        // body lived inline at six `Sexp::Atom(Atom::<variant>(payload))
        // => JValue::<…>(…)` arms; post-lift the outer arm delegates to
        // `a.to_json()` and the per-variant rendering binds at ONE
        // typed-algebra projection on the `Atom` algebra. A regression
        // that drifts the outer arm (e.g. re-inlines ONE variant's
        // rendering without updating `Atom::to_json`, or returns a
        // wrapping `JValue::Array` instead of delegating) surfaces as
        // an inequality here. The cases sweep all six [`AtomKind`]
        // variants. Sibling-arm shape to the quote-family routing test
        // `sexp_to_json_routes_quote_family_arms_through_as_quote_form_typed_marker`
        // and the Display-axis routing test
        // `sexp_atom_display_arm_routes_through_atom_display_for_every_variant`
        // — all three pin the analogous `Sexp` outer arm routing
        // through a typed algebra projection.
        use crate::ast::Atom;
        let cases: &[Atom] = &[
            Atom::Symbol("name".into()),
            Atom::Keyword("kw".into()),
            Atom::Str("body".into()),
            Atom::Int(7),
            Atom::Int(-3),
            Atom::Float(2.5),
            Atom::Float(1.0),
            Atom::Bool(true),
            Atom::Bool(false),
        ];
        for atom in cases {
            let via_sexp = sexp_to_json(&Sexp::Atom(atom.clone()))
                .expect("atom must serialize cleanly through sexp_to_json");
            let via_atom = atom.to_json();
            assert_eq!(
                via_sexp, via_atom,
                "sexp_to_json drifted from Atom::to_json for {atom:?}"
            );
        }
    }

    #[test]
    fn sexp_to_json_float_nan_propagates_atom_to_json_null_branch() {
        // PATH-UNIFORMITY PIN: the float NaN/∞ → `JValue::Null` branch
        // lives at the typed-algebra primitive `Atom::to_json` post-
        // lift; pin that `sexp_to_json` composes through it without
        // an additional wrapping or short-circuit. A regression that
        // added a separate NaN-handling arm at `sexp_to_json`'s outer
        // dispatch (re-introducing the per-callsite branch the lift
        // retires) would diverge here only if the new arm produced a
        // different value than `Atom::to_json` — by sharing the SAME
        // expected output the test catches both kinds of drift
        // (different value at the outer arm; bypassed delegation).
        use crate::ast::Atom;
        for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let atom = Atom::Float(f);
            let via_sexp = sexp_to_json(&Sexp::Atom(atom.clone()))
                .expect("non-finite float atom must serialize cleanly through sexp_to_json");
            assert_eq!(
                via_sexp,
                atom.to_json(),
                "sexp_to_json NaN/∞ branch drifted from Atom::to_json for {atom:?}"
            );
            assert_eq!(
                via_sexp,
                serde_json::Value::Null,
                "non-finite float MUST collapse to JSON Null at the lifted boundary"
            );
        }
    }

    // ── Type-mismatch diagnostics name both expected and got ───────────
    //
    // Every typed extractor's `expected X` message used to leave the operator
    // to inspect the source to discover what kind of value was actually
    // passed. The `expected X, got Y` shape closes that gap: the diagnostic
    // is structurally complete so an authoring surface (REPL, LSP,
    // tatara-check) can render the mismatch without re-reading the input.
    //
    // `sexp_type_name` is the named primitive doing the projection; pinning
    // its outputs here keeps downstream tooling that matches on the names
    // (e.g., "expected string, got int" → squiggly under the int) safe
    // across versions.

    #[test]
    fn sexp_type_name_covers_every_variant() {
        assert_eq!(sexp_type_name(&Sexp::Nil), "nil");
        assert_eq!(sexp_type_name(&Sexp::symbol("foo")), "symbol");
        assert_eq!(sexp_type_name(&Sexp::keyword("k")), "keyword");
        assert_eq!(sexp_type_name(&Sexp::string("s")), "string");
        assert_eq!(sexp_type_name(&Sexp::int(7)), "int");
        assert_eq!(sexp_type_name(&Sexp::float(7.5)), "float");
        assert_eq!(sexp_type_name(&Sexp::boolean(true)), "bool");
        assert_eq!(sexp_type_name(&Sexp::List(vec![])), "list");
        assert_eq!(sexp_type_name(&Sexp::Quote(Box::new(Sexp::Nil))), "quote");
        assert_eq!(
            sexp_type_name(&Sexp::Quasiquote(Box::new(Sexp::Nil))),
            "quasiquote"
        );
        assert_eq!(
            sexp_type_name(&Sexp::Unquote(Box::new(Sexp::Nil))),
            "unquote"
        );
        assert_eq!(
            sexp_type_name(&Sexp::UnquoteSplice(Box::new(Sexp::Nil))),
            "unquote-splice"
        );
    }

    #[test]
    fn sexp_shape_covers_every_variant() {
        // The typed sister to `sexp_type_name_covers_every_variant` —
        // every `Sexp` variant projects to exactly one `SexpShape`
        // variant. After the typed-slot lift the projection's identity
        // is load-bearing data on `LispError::TypeMismatch.got` and
        // `LispError::NamedFormNonSymbolName.got`; pinning the
        // projection here means a regression that drops a `Sexp`
        // variant's typed `SexpShape` mapping fails-loudly. A future
        // `Sexp` extension (e.g. `Sexp::Vector` for `#(...)` reader
        // syntax) would force a `SexpShape` extension AND a new arm
        // in this test, parallel to how `sexp_type_name_covers_every
        // _variant` pins the legacy `&'static str` projection.
        assert_eq!(sexp_shape(&Sexp::Nil), SexpShape::Nil);
        assert_eq!(sexp_shape(&Sexp::symbol("foo")), SexpShape::Symbol);
        assert_eq!(sexp_shape(&Sexp::keyword("k")), SexpShape::Keyword);
        assert_eq!(sexp_shape(&Sexp::string("s")), SexpShape::String);
        assert_eq!(sexp_shape(&Sexp::int(7)), SexpShape::Int);
        assert_eq!(sexp_shape(&Sexp::float(7.5)), SexpShape::Float);
        assert_eq!(sexp_shape(&Sexp::boolean(true)), SexpShape::Bool);
        assert_eq!(sexp_shape(&Sexp::List(vec![])), SexpShape::List);
        assert_eq!(
            sexp_shape(&Sexp::Quote(Box::new(Sexp::Nil))),
            SexpShape::Quote
        );
        assert_eq!(
            sexp_shape(&Sexp::Quasiquote(Box::new(Sexp::Nil))),
            SexpShape::Quasiquote
        );
        assert_eq!(
            sexp_shape(&Sexp::Unquote(Box::new(Sexp::Nil))),
            SexpShape::Unquote
        );
        assert_eq!(
            sexp_shape(&Sexp::UnquoteSplice(Box::new(Sexp::Nil))),
            SexpShape::UnquoteSplice
        );
    }

    #[test]
    fn sexp_shape_routes_quote_family_arms_through_quote_form_sexp_shape_projection() {
        // PATH-UNIFORMITY CONTRACT: the lifted `sexp_shape` routes its
        // four quote-family arms through `Sexp::as_quote_form()` +
        // `QuoteForm::sexp_shape()`. Pin that the legacy per-arm
        // pairing and the typed-projection composition AGREE bit-for-bit
        // across every quote-family `Sexp` shape — a regression in
        // EITHER projection direction (an `as_quote_form` arm that
        // swaps markers, or a `QuoteForm::sexp_shape` arm that drifts
        // its `SexpShape` mapping) surfaces here immediately. Non-
        // quote-family shapes project to `None` from `as_quote_form`
        // and are out of scope for this contract.
        use crate::ast::QuoteForm;
        let samples = [
            (
                Sexp::Quote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Quote,
            ),
            (
                Sexp::Quasiquote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Quasiquote,
            ),
            (
                Sexp::Unquote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Unquote,
            ),
            (
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("payload"))),
                QuoteForm::UnquoteSplice,
            ),
        ];
        for (sexp, expected_qf) in &samples {
            let via_lifted = sexp_shape(sexp);
            let (qf, _) = sexp
                .as_quote_form()
                .expect("quote-family sample must project through as_quote_form");
            assert_eq!(
                qf, *expected_qf,
                "as_quote_form drifted typed marker at {sexp:?}"
            );
            let via_composed = qf.sexp_shape();
            assert_eq!(
                via_lifted, via_composed,
                "sexp_shape drifted from as_quote_form + QuoteForm::sexp_shape at {sexp:?}"
            );
        }
    }

    #[test]
    fn sexp_type_name_delegates_to_sexp_shape_label_for_every_variant() {
        // Pin that the legacy `&'static str` projection and the typed
        // `SexpShape::label()` projection AGREE on every variant —
        // `sexp_type_name(s) == sexp_shape(s).label()` is the
        // bidirection contract that lets the legacy entry point stay
        // pub for tests that match on rendered substrings while new
        // code constructing `LispError::TypeMismatch` /
        // `NamedFormNonSymbolName` passes through `sexp_shape`
        // directly. A regression that drifts either projection
        // (e.g. typo in `SexpShape::label()` arm, change in
        // `sexp_type_name`'s match) fails-loudly here.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                sexp_type_name(s),
                sexp_shape(s).label(),
                "sexp_type_name and sexp_shape(_).label() must agree for {s:?}"
            );
        }
    }

    #[test]
    fn sexp_witness_pairs_typed_shape_with_display_projection() {
        // Pin the typed joint-identity contract: `sexp_witness(&sexp)`
        // produces a `SexpWitness` whose `shape` is `sexp_shape(&sexp)`
        // AND whose `display` is `sexp.to_string()`. The helper is the
        // single primitive that bundles both halves of the offending-
        // value identity into one owned typed value — every variant
        // slot that takes a `SexpWitness` (currently
        // `SpliceOutsideList.got`; future moves: `NonSymbolUnquoteTarget`,
        // `NonSymbolParam`, `RestParamMissingName`, `DefmacroNonSymbolName`,
        // `DefmacroNonListParams`, `MissingHeadSymbol`) routes through
        // this primitive at the helper boundary. A regression that
        // drops either projection (shape or display) at the helper
        // boundary fails-loudly here.
        let w = sexp_witness(&Sexp::int(5));
        assert_eq!(w.shape, SexpShape::Int);
        assert_eq!(w.display, "5");

        let w = sexp_witness(&Sexp::symbol("notify-ref"));
        assert_eq!(w.shape, SexpShape::Symbol);
        assert_eq!(w.display, "notify-ref");

        let w = sexp_witness(&Sexp::keyword("foo"));
        assert_eq!(w.shape, SexpShape::Keyword);
        assert_eq!(w.display, ":foo");

        let w = sexp_witness(&Sexp::List(vec![
            Sexp::symbol("list"),
            Sexp::int(1),
            Sexp::int(2),
        ]));
        assert_eq!(w.shape, SexpShape::List);
        assert_eq!(w.display, "(list 1 2)");

        let w = sexp_witness(&Sexp::Nil);
        assert_eq!(w.shape, SexpShape::Nil);
        assert_eq!(w.display, "()");
    }

    #[test]
    fn sexp_witness_distinguishes_int_atom_from_symbol_with_same_display() {
        // Pin the structural bifurcation between two `Sexp`s whose
        // `Display` projection is the same string but whose typed
        // `SexpShape` differs. `Sexp::int(5).to_string() == "5"`
        // AND `Sexp::symbol("5").to_string() == "5"` (the reader
        // would reject the symbol `5`, but the AST allows it — the
        // bifurcation here pins that `sexp_witness` carries the
        // structural shape so tools can distinguish them even when
        // the rendered literal is identical). A regression that
        // drops the typed shape from `SexpWitness` would collapse
        // this distinction.
        let w_int = sexp_witness(&Sexp::int(5));
        let w_sym = sexp_witness(&Sexp::symbol("5"));
        assert_eq!(w_int.display, w_sym.display);
        assert_ne!(w_int.shape, w_sym.shape);
        assert_eq!(w_int.shape, SexpShape::Int);
        assert_eq!(w_sym.shape, SexpShape::Symbol);
    }

    fn type_err_message(err: LispError) -> String {
        format!("{err}")
    }

    #[test]
    fn extract_string_type_err_names_got_int() {
        let args = kwargs_of("(_ :name 42)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string(&kw, "name").unwrap_err());
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
        assert!(msg.contains(":name"), "got: {msg}");
    }

    #[test]
    fn extract_optional_string_type_err_names_got_bool() {
        let args = kwargs_of("(_ :name #t)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_string(&kw, "name").unwrap_err());
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got bool"), "got: {msg}");
    }

    #[test]
    fn extract_int_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :n "seven")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_int(&kw, "n").unwrap_err());
        assert!(msg.contains("expected int"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_float_type_err_names_got_bool() {
        let args = kwargs_of("(_ :ratio #f)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_float(&kw, "ratio").unwrap_err());
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got bool"), "got: {msg}");
    }

    #[test]
    fn extract_bool_type_err_names_got_int() {
        let args = kwargs_of("(_ :enabled 1)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_bool(&kw, "enabled").unwrap_err());
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_type_err_on_scalar_names_got_string() {
        // `:tags "scalar"` — list-typed kwarg given a scalar. The error
        // names the actual shape so the operator sees the mismatch
        // structurally.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string_list(&kw, "tags").unwrap_err());
        assert!(msg.contains("expected list of strings"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_type_err_on_non_string_item_names_index_and_got_int() {
        // `:tags ("ok" 7)` — outer is a list, the second item isn't a
        // string. Diagnostic names BOTH the item path (`:tags[1]`) and the
        // narrower per-item expectation (`expected string`, not the outer
        // `expected list of strings`) so authors see structurally where
        // the failure is, not just which kwarg.
        let args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string_list(&kw, "tags").unwrap_err());
        assert!(
            msg.contains(":tags[1]"),
            "expected indexed item path, got: {msg}"
        );
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_optional_int_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :n "seven")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_int(&kw, "n").unwrap_err());
        assert!(msg.contains("expected int"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_optional_float_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :ratio "half")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_float(&kw, "ratio").unwrap_err());
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_optional_bool_type_err_names_got_int() {
        let args = kwargs_of("(_ :enabled 1)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_bool(&kw, "enabled").unwrap_err());
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_non_list_kwarg_names_got_string() {
        // `:steps "scalar"` — the vec-fallthrough's "expected list" used
        // to be a bare label; now it also reports the actual outer shape.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg =
            type_err_message(extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err());
        assert!(msg.contains("expected list"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn derive_type_err_end_to_end_names_got_string_for_threshold() {
        // End-to-end through `#[derive(TataraDomain)]`. A misspelled-as-
        // string `:threshold "tight"` used to surface as "expected
        // number" with no signal what was actually passed; now the
        // diagnostic carries `got string` so authoring surfaces have
        // structural info to render without re-reading the source.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold "tight")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":threshold"), "got: {msg}");
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    // ── Odd-kwargs dangling-element naming ─────────────────────────────
    //
    // `(defX :name "x" :query)` used to surface as the bare "odd number of
    // keyword arguments" message — operator could not tell whether
    // `:query`'s value got lost or whether the form was malformed. The
    // structural fix names the dangling element via `Sexp::Display`:
    //   - keyword case (`:query` with no value) → `:query`
    //   - non-keyword case (stray `5` at tail)  → `5`
    // Both halves of the failure are now structurally complete: the gate
    // names the failure mode AND the offending element. Pinning each case
    // here keeps `tatara-check` / LSP / REPL renderings safe across
    // versions, and means a future run that gives `Sexp` source spans
    // attaches a position to the same single primitive (`OddKwargs`)
    // mechanically.
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry); §V.1
    // (knowable platform — diagnostic names both expected and actual).

    #[test]
    fn parse_kwargs_names_dangling_keyword() {
        // `:name "x" :query` — `:query` has no value. The error variant
        // carries the dangling kwarg's display, so the author sees which
        // keyword lost its value.
        let args = kwargs_of(r#"(_ :name "x" :query)"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == ":query"),
            "expected OddKwargs {{ dangling: \":query\" }}, got {err:?}"
        );
        assert!(
            msg.contains(":query"),
            "error must name the dangling keyword, got: {msg}"
        );
        assert!(
            msg.contains("dangling"),
            "expected 'dangling' in the message, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_names_dangling_non_keyword_scalar() {
        // `:name "x" :query "q" 5` — a stray scalar at the tail. The
        // dangling element's `Sexp::Display` is `5`; the diagnostic must
        // name it so the author knows what to delete (or which kwarg key
        // to add in front of it).
        let args = kwargs_of(r#"(_ :name "x" :query "q" 5)"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == "5"),
            "expected OddKwargs {{ dangling: \"5\" }}, got {err:?}"
        );
        assert!(
            msg.contains('5'),
            "error must name the dangling scalar, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_names_dangling_string_scalar() {
        // `:name "x" "stray"` — a stray string at the tail. The Sexp
        // Display projects strings through `{:?}`, so the diagnostic
        // contains the quoted form `"stray"` — preserves the typed shape.
        let args = kwargs_of(r#"(_ :name "x" "stray")"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == "\"stray\""),
            "expected OddKwargs {{ dangling: \"\\\"stray\\\"\" }}, got {err:?}"
        );
        assert!(
            msg.contains("stray"),
            "error must name the dangling string, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_single_dangling_keyword() {
        // `(_ :only)` — a single dangling keyword with nothing else. The
        // gate must name it the same way as the multi-kwarg case;
        // structural completeness should not depend on list length.
        let args = kwargs_of("(_ :only)");
        let err = parse_kwargs(&args).unwrap_err();
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == ":only"),
            "expected OddKwargs {{ dangling: \":only\" }}, got {err:?}"
        );
    }

    #[test]
    fn derive_odd_kwargs_end_to_end_names_dangling_keyword() {
        // End-to-end through `#[derive(TataraDomain)]`. A truncated
        // authoring form `(defmonitor :name "x" :query)` used to surface
        // as a bare "odd number" message; now every derived domain
        // inherits the named-dangling-element diagnostic for free
        // because they all funnel through `parse_kwargs`.
        let forms = read(r#"(defmonitor :name "x" :query)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":query"),
            "derived odd-kwargs error must name the dangling kwarg, got: {msg}"
        );
        assert!(
            msg.contains("dangling"),
            "expected 'dangling' label end-to-end, got: {msg}"
        );
    }

    // ── Indexed-item form labels for list-typed kwargs ─────────────────
    //
    // `kwarg_form` and `kwarg_item_form` are the two named primitives
    // that build the `form:` field of every typed-entry diagnostic. The
    // base helper consolidates seven inline `format!(":{key}")` copies
    // (parse_kwargs duplicate, reject_unknown_kwargs, required, type_err,
    // deserialize_err, sexp_to_json's nested-duplicate, the non-list
    // path in extract_vec_via_serde) into one site; the indexed helper
    // adds the structural slot for *which item* failed.
    //
    // Pinning the canonical shapes here keeps downstream tooling
    // (`tatara-check`, LSP, REPL) safe across versions, and means a
    // future run that gives `Sexp` source spans threads `pos` through
    // ONE primitive instead of every macro emit. Frontier inspiration:
    // JSON Pointer (`/steps/1`), jq paths.

    #[test]
    fn kwarg_form_renders_canonical_shape() {
        // After the typed-slot promotion the helpers return `KwargPath`
        // (the typed enum, structurally bound) rather than `String`;
        // Display projects each variant to its canonical literal
        // byte-for-byte equivalent to the legacy `format!` shape. Pin
        // both the structural identity AND the rendered literal so the
        // dual contract (typed-binding + byte-for-byte display) is
        // anchored from both angles.
        assert_eq!(
            kwarg_form("threshold"),
            crate::error::KwargPath::Named("threshold".into())
        );
        assert_eq!(kwarg_form("threshold").to_string(), ":threshold");
        assert_eq!(kwarg_form("notify-ref").to_string(), ":notify-ref");
        // No transformation of the key — the surface name is what the
        // author sees in the source. `kebab_to_camel` happens elsewhere.
        assert_eq!(kwarg_form("").to_string(), ":");
    }

    #[test]
    fn kwarg_item_form_renders_canonical_indexed_shape() {
        assert_eq!(
            kwarg_item_form("tags", 0),
            crate::error::KwargPath::Item {
                key: "tags".into(),
                idx: 0
            }
        );
        assert_eq!(kwarg_item_form("tags", 0).to_string(), ":tags[0]");
        assert_eq!(kwarg_item_form("steps", 1).to_string(), ":steps[1]");
        assert_eq!(kwarg_item_form("steps", 17).to_string(), ":steps[17]");
    }

    #[test]
    fn kwargs_pos_form_renders_canonical_slot_shape() {
        // Sibling of `kwarg_form` / `kwarg_item_form` — used when the
        // kwargs slot itself failed the keyword gate, so there is no
        // `:<key>` to root the path. Pin both the structural identity
        // (`KwargPath::Slot(i)`) AND the rendered literal
        // (`kwargs[<idx>]`) so `tatara-check` / LSP / REPL match either
        // surface directly.
        assert_eq!(kwargs_pos_form(0), crate::error::KwargPath::Slot(0));
        assert_eq!(kwargs_pos_form(0).to_string(), "kwargs[0]");
        assert_eq!(kwargs_pos_form(2).to_string(), "kwargs[2]");
        assert_eq!(kwargs_pos_form(42).to_string(), "kwargs[42]");
    }

    #[test]
    fn extract_string_list_outer_failure_keeps_unindexed_form() {
        // Negative-control: the outer-shape failure (`:tags "scalar"`)
        // is at the kwarg level, not the item level — its form must NOT
        // pick up an `[idx]` suffix, and the message keeps the wider
        // `expected list of strings`.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":tags"), "got: {msg}");
        assert!(
            !msg.contains(":tags["),
            "outer failure must not gain an item index, got: {msg}"
        );
        assert!(msg.contains("expected list of strings"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_indexes_each_failing_item() {
        // The first non-string item wins (collect short-circuits on the
        // first Err). Pin the index math: a failure at position 2 must
        // surface as `:tags[2]`, not `:tags[0]` or `:tags[1]`.
        let args = kwargs_of(r#"(_ :tags ("ok" "also-ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!("{}", extract_string_list(&kw, "tags").unwrap_err());
        assert!(msg.contains(":tags[2]"), "got: {msg}");
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_indexes_failing_item() {
        // Second item has a non-string `:notify-ref`. The serde error
        // must surface under `:steps[1]` so the operator goes straight
        // to the bad item — previously the index was lost and the
        // diagnostic only named `:steps`. Bind on the structural
        // variant: `idx: Some(1)` makes the index addressable as
        // first-class data, not a substring of the rendered message.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "ok")
                  (:notify-ref 7)))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { ref key, idx: 1 },
                    ..
                } if key == "steps"
            ),
            "expected KwargDeserialize {{ path: KwargPath::Item {{ key: \"steps\", idx: 1 }}, .. }}, got {err:?}"
        );
        let msg = format!("{err}");
        assert!(msg.contains(":steps[1]"), "got: {msg}");
        assert!(msg.contains("deserialize:"), "got: {msg}");
    }

    #[test]
    fn extract_optional_via_serde_deserialize_failure_emits_kwarg_deserialize_variant() {
        // `:level NotASeverity` — well-formed Sexp, ill-formed enum.
        // The optional path must NOT short-circuit when the kwarg IS
        // present but malformed; it must produce the same structural
        // `LispError::KwargDeserialize { path: KwargPath::Named(_), .. }`
        // variant the required path produces, so the typed-entry
        // `from_value` rejection mode is uniform across the required +
        // optional pair — `extract_via_serde` and
        // `extract_optional_via_serde` share ONE error path via
        // `deserialize_err`.
        let args = kwargs_of("(_ :level NotASeverity)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_optional_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(ref key),
                    ref message,
                } if key == "level" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Named(\"level\"), .. }}, got {err:?}"
        );
    }

    #[test]
    fn from_value_with_path_threads_typed_kwarg_path_into_kwarg_deserialize_variant() {
        // The three-times-rule lift's load-bearing pin:
        // `from_value_with_path::<T>(sexp, path)` is THE primitive every
        // extractor that crosses the typed-entry JSON boundary funnels
        // through. The primitive's variant slot is the typed
        // `LispError::KwargDeserialize { path: KwargPath, message }` — the
        // path identity threads from the caller verbatim into the
        // variant's typed slot, so `KwargPath::Named` from
        // `extract_via_serde` / `extract_optional_via_serde` AND
        // `KwargPath::Item { key, idx }` from `extract_vec_via_serde`'s
        // per-item closure both ride ONE primitive's `map_err` arm, not
        // three site-specific shims. Pin both path-shape arms in one
        // test so the load-bearing data-shape symmetry is anchored at
        // the primitive's boundary (not just at the three call sites
        // separately, which the prior tests already cover end-to-end).
        // A regression that re-introduces a sibling shim (collapsing
        // the typed `KwargPath` slot back into a `(key, idx:
        // Option<usize>)` pair at the helper boundary, the pre-33c64c9
        // shape) fails-loudly here AND at the existing extractor-
        // boundary tests.

        // Named-path arm: a malformed enum value flows through
        // `from_value_with_path` with `KwargPath::named("level")` into
        // the typed variant slot — same path identity an
        // `extract_via_serde::<Severity>(kw, "level")` would thread.
        let bad = Sexp::symbol("NotASeverity");
        let err = from_value_with_path::<Severity>(&bad, KwargPath::named("level"))
            .expect_err("malformed enum value must error");
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(ref key),
                    ref message,
                } if key == "level" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Named(\"level\"), .. }}, got {err:?}"
        );

        // Item-path arm: a per-item failure flows through the SAME
        // primitive with `KwargPath::item("steps", 1)` — the per-item
        // sub-mode of the same JSON-projection rejection chain. The
        // primitive's `map_err` arm threads the typed `KwargPath::Item
        // { key, idx }` into the variant's typed slot byte-for-byte,
        // bifurcating from the Named-arm above by variant identity (not
        // by a sibling `idx: Option<usize>` slot).
        let bad_item = Sexp::int(7);
        let err_item =
            from_value_with_path::<EscalationStep>(&bad_item, KwargPath::item("steps", 1))
                .expect_err("malformed item must error");
        assert!(
            matches!(
                err_item,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { ref key, idx: 1 },
                    ..
                } if key == "steps"
            ),
            "expected KwargDeserialize {{ path: KwargPath::Item {{ key: \"steps\", idx: 1 }}, .. }}, got {err_item:?}"
        );

        // Display preserves the legacy byte-for-byte shape across both
        // path identities — `compile error in :level: deserialize: …`
        // for the named arm, `compile error in :steps[1]: deserialize: …`
        // for the item arm. The substring-grep contract that
        // `tatara-check` / REPL relied on pre-lift passes through the
        // new primitive's `LispError::Display` projection unchanged.
        let msg = format!("{err}");
        assert!(
            msg.contains(":level"),
            "named display must name kwarg, got: {msg}"
        );
        assert!(msg.contains("deserialize:"), "got: {msg}");
        let msg_item = format!("{err_item}");
        assert!(
            msg_item.contains(":steps[1]"),
            "item display must name kwarg+idx, got: {msg_item}"
        );
        assert!(msg_item.contains("deserialize:"), "got: {msg_item}");
    }

    #[test]
    fn kwarg_deserialize_helpers_share_variant_across_scalar_and_per_item_paths() {
        // Type-bound symmetry: `extract_via_serde` (scalar / required)
        // AND `extract_vec_via_serde` (per-item) BOTH funnel through
        // the SAME structural variant — `LispError::KwargDeserialize` —
        // bifurcated by `KwargPath::Named` vs. `KwargPath::Item`
        // variant identity. Pin both paths in ONE test so the symmetry
        // is load-bearing in the type system: a regression that drifts
        // either site to a different variant fails-loudly here. Mirror
        // at the typed-entry-side of the typed-exit-side
        // `helpers_are_type_bound_via_t_keyword` symmetry test (which
        // pins `register::<T>` AND `rewrite_typed::<T>` BOTH route
        // through `DomainSerialize`).
        let args = kwargs_of("(_ :level NotASeverity)");
        let kw = parse_kwargs(&args).unwrap();
        let scalar_err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(
                scalar_err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(_),
                    ..
                }
            ),
            "scalar path must produce KwargDeserialize with KwargPath::Named, got {scalar_err:?}"
        );

        let args = kwargs_of(r#"(_ :steps ((:notify-ref 7)))"#);
        let kw = parse_kwargs(&args).unwrap();
        let item_err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                item_err,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { idx: 0, .. },
                    ..
                }
            ),
            "per-item path must produce KwargDeserialize with KwargPath::Item, got {item_err:?}"
        );
    }

    #[test]
    fn extract_vec_via_serde_outer_failure_keeps_unindexed_form() {
        // Negative-control: the outer kwarg-isn't-a-list failure stays
        // at `:steps` (no `[N]`). The wider `expected list` message is
        // preserved.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!(
            "{}",
            extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err()
        );
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(
            !msg.contains(":steps["),
            "outer failure must not gain an item index, got: {msg}"
        );
        assert!(msg.contains("expected list"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_propagates_inner_duplicate_with_inner_form() {
        // Inner `(:notify-ref "a" :notify-ref "b")` fails inside
        // `sexp_to_json` BEFORE `serde_json::from_value` runs — that
        // path's error already carries its own `form: ":notify-ref"`,
        // and the item-level wrapper must not clobber it with
        // `:steps[0]`. Pin the propagation: the operator sees the
        // duplicated inner kwarg, not just the item index.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!(
            "{}",
            extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err()
        );
        assert!(msg.contains(":notify-ref"), "got: {msg}");
        assert!(msg.contains("duplicate keyword"), "got: {msg}");
    }

    #[test]
    fn derive_indexed_item_failure_e2e_via_monitor_tags() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // `:tags ("prod" 7)` must surface as `:tags[1]` so every
        // derived domain inherits the indexed-item diagnostic by
        // sharing `extract_string_list` — no per-derive macro change.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tags ("prod" 7))"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":tags[1]"),
            "derived item-failure error must name the index, got: {msg}"
        );
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn parse_kwargs_well_formed_input_is_unaffected() {
        // Negative-control: even-length kwargs lists with no duplicates
        // and no unknowns continue to parse identically. The dangling-
        // element gate must NOT false-positive on canonical input.
        let args = kwargs_of(r#"(_ :name "x" :query "q" :threshold 0.5)"#);
        let kw = parse_kwargs(&args).expect("well-formed kwargs must parse");
        assert_eq!(kw.len(), 3);
    }

    // ── Structural TypeMismatch for not-a-keyword-at-position ──────────
    //
    // `parse_kwargs` used to raise a `LispError::Compile { form: "kwargs",
    // message: format!("expected keyword at position {i}") }` triple when
    // an even-position element wasn't a keyword. Three problems:
    //   1. `form: "kwargs"` is a generic label — operators couldn't tell
    //      which slot misfired without re-counting.
    //   2. The actual got-type was lost; `(_ "x" 5)` and `(_ 5 "x")` and
    //      `(_ #t 5)` all rendered the same "expected keyword at position
    //      0" message.
    //   3. The diagnostic was structurally distinct from every other
    //      typed-entry mismatch in the substrate (`TypeMismatch`),
    //      forcing authoring tools to substring-parse instead of binding
    //      to the variant.
    //
    // Lifting into `type_mismatch(kwargs_pos_form(i), "keyword",
    // &args[i])` collapses all three: the form is `kwargs[<idx>]`, the
    // got-type is the structural `sexp_type_name(_)` projection, and the
    // variant is the same `LispError::TypeMismatch` that
    // `extract_string` / `extract_int` / etc. already produce.
    //
    // Theory anchor: THEORY.md §V.1 (knowable platform — the diagnostic
    // names both expected AND actual); §VI.1 (generation over
    // composition — one `LispError::TypeMismatch` variant for every
    // kwarg-shape failure mode).

    #[test]
    fn parse_kwargs_non_keyword_at_position_0_emits_type_mismatch_variant() {
        // `(_ "x" 5)` — args[0] is a string, not a keyword. The variant
        // must be `TypeMismatch`, not the legacy `Compile`. `expected`
        // is the typed `ExpectedKwargShape` enum, so a typo in the static
        // label can never drift; `got` is the typed `SexpShape` enum
        // sourced from `sexp_shape(_)`'s exhaustive projection over
        // `Sexp`'s closed set of 12 outermost shapes.
        let args = kwargs_of(r#"(_ "x" 5)"#);
        let err = parse_kwargs(&args).expect_err("non-keyword position must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    form: crate::error::KwargPath::Slot(0),
                    expected: ExpectedKwargShape::Keyword,
                    got: SexpShape::String,
                }
            ),
            "expected TypeMismatch {{ form: KwargPath::Slot(0), expected: Keyword, got: SexpShape::String }}, got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_at_position_2_emits_type_mismatch_variant() {
        // `(_ :name "x" "y" 5)` — first pair `:name "x"` succeeds; second
        // pair starts at position 2 with a string. The form must name
        // `kwargs[2]` so the operator goes straight to the slot — pin the
        // index math via the typed `KwargPath::Slot(2)` identity.
        let args = kwargs_of(r#"(_ :name "x" "y" 5)"#);
        let err = parse_kwargs(&args).expect_err("non-keyword at later position must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    form: crate::error::KwargPath::Slot(2),
                    expected: ExpectedKwargShape::Keyword,
                    got: SexpShape::String,
                }
            ),
            "expected indexed TypeMismatch at KwargPath::Slot(2), got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_routes_got_through_sexp_type_name() {
        // The got-type is the structural `sexp_shape(_)` projection,
        // not a free-form string — pinning this contract for ints, bools,
        // and symbols means a regression that re-inlines the diagnostic
        // (with `format!("got {}", _)`) fails-loudly here. Three shapes
        // covered: int, bool, symbol — each routes through the typed
        // projection.
        let args = kwargs_of(r#"(_ 5 "v")"#);
        let err = parse_kwargs(&args).expect_err("int at position 0 must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    got: SexpShape::Int,
                    ..
                }
            ),
            "expected got: SexpShape::Int, got {err:?}"
        );

        let args = kwargs_of(r#"(_ #t "v")"#);
        let err = parse_kwargs(&args).expect_err("bool at position 0 must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    got: SexpShape::Bool,
                    ..
                }
            ),
            "expected got: SexpShape::Bool, got {err:?}"
        );

        let args = kwargs_of(r#"(_ symbolic "v")"#);
        let err = parse_kwargs(&args).expect_err("symbol at position 0 must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    got: SexpShape::Symbol,
                    ..
                }
            ),
            "expected got: SexpShape::Symbol, got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_message_renders_canonical_type_mismatch_shape() {
        // Display matches the standard TypeMismatch render — `compile
        // error in kwargs[0]: expected keyword, got string` — so
        // authoring tools that already substring-match on `expected …,
        // got …` (`tatara-check` / LSP / REPL) light up uniformly for
        // this slot the way they do for kwarg-level type mismatches.
        let args = kwargs_of(r#"(_ "x" 5)"#);
        let err = parse_kwargs(&args).expect_err("must error");
        assert_eq!(
            format!("{err}"),
            "compile error in kwargs[0]: expected keyword, got string"
        );
    }

    #[test]
    fn derive_non_keyword_at_position_e2e_via_monitor() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // `(defmonitor "stray" :name …)` — first kwargs element is a
        // stray string, not a keyword. The derived path inherits the lift
        // for free because every derived domain funnels through
        // `parse_kwargs`; no per-derive macro change.
        let forms = read(r#"(defmonitor "stray" :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    form: crate::error::KwargPath::Slot(0),
                    expected: ExpectedKwargShape::Keyword,
                    got: SexpShape::String,
                }
            ),
            "expected derived TypeMismatch at KwargPath::Slot(0), got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_position_is_none_today() {
        // Negative-control for the future-spans move: until `Sexp`
        // carries source positions, the variant's `position()` returns
        // `None`. Pinning this contract means a future run that adds
        // `pos: Option<usize>` to `TypeMismatch` does so with a fail-
        // before/pass-after delta — and the not-a-keyword path picks up
        // the span automatically because it routes through the same
        // primitive (`type_mismatch`) as every other `TypeMismatch` site.
        let args = kwargs_of(r#"(_ "x" 5)"#);
        let err = parse_kwargs(&args).expect_err("must error");
        assert_eq!(err.position(), None);
    }

    // ── Structural TypeMismatch variant ────────────────────────────────
    //
    // The three "expected X, got Y" sites in this module — `type_err`,
    // `extract_string_list` per-item, `extract_vec_via_serde` non-list —
    // used to assemble the message inline via three near-identical
    // `format!("expected {expected}, got {}", sexp_type_name(_))` copies.
    // Three copies is the THEORY.md §VI.1 three-times-rule signal.
    //
    // `LispError::TypeMismatch { form, expected, got }` collapses the
    // shape into one structural variant: `form` is the path slot
    // (`kwarg_form` or `kwarg_item_form`), `expected` is the static
    // expectation, `got` is the static `sexp_type_name` projection.
    // Authoring tools (REPL, LSP, `tatara-check`) bind to the variant
    // directly instead of substring-parsing a rendered message; rendered
    // text matches the legacy `Compile`-shaped diagnostic byte-for-byte,
    // so existing `msg.contains("expected …")` assertions pass.
    //
    // Pinning the variant identity here keeps the structural binding
    // safe across versions, and means a future run that gives `Sexp`
    // source spans threads `pos: Option<usize>` through ONE primitive
    // (`type_mismatch`) — every type-mismatch site picks up positional
    // rendering with no consumer changes.

    #[test]
    fn type_mismatch_helper_emits_structured_variant() {
        // `type_mismatch` now takes a typed `KwargPath` for `form` AND
        // a typed `ExpectedKwargShape` for `expected` — pin the
        // structural identity of every slot, including that BOTH typed
        // enums are threaded into the variant byte-identically (not
        // coerced through a String round-trip).
        let err = type_mismatch(kwarg_form("ctx"), ExpectedKwargShape::String, &Sexp::int(7));
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("ctx".into()));
                assert_eq!(expected, ExpectedKwargShape::String);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn type_mismatch_display_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { message: format!("expected …, got …") }`
        // rendering. Authoring surfaces that pattern-match on the message
        // text continue to work; tools that pattern-match on the variant
        // gain structural binding.
        let err = type_mismatch(
            kwarg_form("threshold"),
            ExpectedKwargShape::Number,
            &Sexp::string("tight"),
        );
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected number, got string"
        );
    }

    #[test]
    fn extract_string_returns_type_mismatch_variant() {
        // The kwarg-level `expected X, got Y` site now produces the
        // structural variant. Pin the variant identity AND the rendered
        // message so the substrate's contract is locked from both
        // angles.
        let args = kwargs_of("(_ :name 42)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string(&kw, "name").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::String,
                    got: SexpShape::Int,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "name")
            ),
            "expected TypeMismatch {{ form: KwargPath::Named(\"name\"), expected: String, got: SexpShape::Int }}, got {err:?}"
        );
        assert_eq!(
            format!("{err}"),
            "compile error in :name: expected string, got int"
        );
    }

    #[test]
    fn extract_string_list_per_item_returns_indexed_type_mismatch() {
        // Per-item failure in a `Vec<String>` kwarg flows through
        // `type_err_at` → `kwarg_item_form` + `type_mismatch`. Pin the
        // typed `KwargPath::Item { key: "tags", idx: 1 }` identity
        // directly (no String round-trip).
        let args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::String,
                    got: SexpShape::Int,
                } if matches!(form, crate::error::KwargPath::Item { key, idx: 1 } if key == "tags")
            ),
            "expected indexed TypeMismatch at KwargPath::Item {{ key: \"tags\", idx: 1 }}, got {err:?}"
        );
    }

    #[test]
    fn extract_vec_via_serde_non_list_returns_type_mismatch() {
        // The vec-fallthrough's "expected list" path lifts into the
        // same variant — `:steps "scalar"` no longer produces
        // `LispError::Compile`; it produces `TypeMismatch` with
        // `form: KwargPath::Named("steps")`, `expected: "list"`,
        // `got: "string"`. Authoring tools see the same shape regardless
        // of which extractor reported the mismatch.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::List,
                    got: SexpShape::String,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "steps")
            ),
            "expected list-shape TypeMismatch at KwargPath::Named(\"steps\"), got {err:?}"
        );
    }

    #[test]
    fn extract_string_list_outer_failure_returns_list_of_strings_type_mismatch() {
        // The outer-shape failure (`:tags "scalar"`) is at the kwarg
        // level — its `expected` stays `"list of strings"` (wider than
        // the per-item case's `"string"`) and the form has no `[idx]`
        // suffix (`KwargPath::Named`, not `KwargPath::Item`). Same
        // variant; different `expected` + path-shape.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::ListOfStrings,
                    got: SexpShape::String,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "tags")
            ),
            "expected outer-shape TypeMismatch at KwargPath::Named(\"tags\"), got {err:?}"
        );
    }

    #[test]
    fn type_mismatch_position_is_none_today() {
        // Negative-control: until `Sexp` carries spans, `position()`
        // returns `None` for the variant — `format_diagnostic` falls
        // through to single-line rendering, no caret emitted. Pinning
        // this contract means a future run that adds `pos: Option<usize>`
        // does so deliberately, with a fail-before/pass-after delta.
        let err = type_mismatch(kwarg_form("x"), ExpectedKwargShape::String, &Sexp::int(0));
        assert_eq!(err.position(), None);
    }

    #[test]
    fn derive_type_mismatch_e2e_via_monitor_threshold() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // a misspelled-as-string `:threshold "tight"` surfaces the
        // structural variant. Every derived domain inherits the lift —
        // no per-derive macro change.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold "tight")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::Number,
                    got: SexpShape::String,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "threshold")
            ),
            "expected derived TypeMismatch, got {err:?}"
        );
    }

    // ── compile_from_sexp form-shape primitives ───────────────────────

    #[test]
    fn head_mismatch_emits_structural_variant() {
        let err = head_mismatch("defmonitor", "not-a-monitor".into());
        assert!(
            matches!(
                err,
                LispError::HeadMismatch {
                    keyword: "defmonitor",
                    ref got,
                } if got == "not-a-monitor"
            ),
            "expected HeadMismatch variant, got {err:?}"
        );
    }

    #[test]
    fn head_mismatch_display_matches_legacy_compile_shape() {
        // Legacy shape (before this lift):
        //   "compile error in defmonitor: expected (defmonitor ...), got (X ...)"
        // The structural variant must render byte-for-byte the same so
        // existing consumer assertions (e.g. `tatara-check`, the
        // `derive_errors_on_wrong_head` test) pass unchanged.
        let err = head_mismatch("defmonitor", "not-a-monitor".into());
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: expected (defmonitor ...), got (not-a-monitor ...)"
        );
    }

    #[test]
    fn not_a_list_form_err_emits_structural_variant() {
        // After the structural lift the helper returns
        // `LispError::NotAListForm { keyword }`, not the legacy
        // `Compile { form, message }` triple. Pinning variant
        // identity (rather than substring-matching on `message ==
        // "expected list form"`) means a regression that revives
        // the `Compile`-shaped construction fails-loudly here.
        let err = not_a_list_form_err("defmonitor");
        assert!(
            matches!(
                err,
                LispError::NotAListForm {
                    keyword: "defmonitor"
                }
            ),
            "expected NotAListForm variant, got {err:?}"
        );
    }

    #[test]
    fn not_a_list_form_err_display_matches_legacy_compile_shape() {
        // Legacy shape (before this lift):
        //   "compile error in defmonitor: expected list form"
        // The structural variant must render byte-for-byte the
        // same so existing consumer assertions (e.g., the
        // `compile_from_sexp_emits_*_for_non_list_form` tests
        // against `MonitorSpec`, `tatara-check`'s diagnostic
        // capture, REPL substring matchers) pass unchanged.
        let err = not_a_list_form_err("defmonitor");
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: expected list form"
        );
    }

    #[test]
    fn missing_head_err_with_no_got_returns_missing_head_symbol_for_empty_list() {
        // The empty-list case (`()`) — `list.first()` returns `None`,
        // so the call site passes `got: None`. The builder returns
        // `LispError::MissingHeadSymbol { keyword, got: None }`
        // structurally — a regression that re-collapsed both sub-
        // modes into the legacy `Compile` shape would fail-loudly
        // here. Display-side coverage of the rendered message lives
        // in `tatara-lisp/src/error.rs`'s test module.
        let err = missing_head_err("defmonitor", None);
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    got: None,
                }
            ),
            "expected MissingHeadSymbol {{ got: None }}, got {err:?}"
        );
    }

    #[test]
    fn missing_head_err_with_got_returns_missing_head_symbol_for_non_symbol_head() {
        // The present-but-not-symbol case (`(5 …)`, `(:foo …)`) —
        // `list.first()` returns `Some(non-symbol-sexp)`, so the
        // call site passes `got: Some(SexpWitness)`. The builder
        // returns `LispError::MissingHeadSymbol { keyword, got:
        // Some(_) }` structurally so the renderable detail names
        // the offending head, parallel to how
        // `RestParamMissingName.got: Some(_)` names the offending
        // post-`&rest` follower. The typed witness carries the
        // joint (`SexpShape::Int`, "5") identity so authoring tools
        // bind to `got.shape` directly across the rejection slot.
        let err = missing_head_err("defmonitor", Some(SexpWitness::new(SexpShape::Int, "5")));
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    ref got,
                } if got.as_ref().map(|w| (w.shape, w.display.as_str())) == Some((SexpShape::Int, "5"))
            ),
            "expected MissingHeadSymbol {{ got: Some(SexpWitness {{ Int, \"5\" }}) }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_head_mismatch_for_wrong_head() {
        // End-to-end through the trait default: a `(not-a-monitor …)`
        // form fed to `MonitorSpec::compile_from_sexp` surfaces the
        // structural HeadMismatch — every derived domain (and every
        // hand-written impl that uses the trait default) inherits.
        let forms = read(r#"(not-a-monitor :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::HeadMismatch {
                    keyword: "defmonitor",
                    ref got,
                } if got == "not-a-monitor"
            ),
            "expected HeadMismatch, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_not_a_list_form_for_bare_atom() {
        // End-to-end through the trait default: a bare-atom form
        // (no parens) fed to `MonitorSpec::compile_from_sexp`
        // surfaces the structural `NotAListForm` variant — every
        // derived domain (and every hand-written impl that uses
        // the trait default) inherits the structural gate.
        let err = MonitorSpec::compile_from_sexp(&Sexp::int(7)).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NotAListForm {
                    keyword: "defmonitor"
                }
            ),
            "expected NotAListForm, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_not_a_list_form_for_keyword_atom() {
        // A keyword atom (`:foo`) is also a non-list — pin path-
        // uniformity across atom kinds. The keyword projection in
        // the variant doesn't change with the offending atom's
        // type because `NotAListForm` carries no `got` slot — the
        // failure mode IS "not a list", regardless of what kind
        // of atom was supplied.
        let err = MonitorSpec::compile_from_sexp(&Sexp::keyword("foo")).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NotAListForm {
                    keyword: "defmonitor"
                }
            ),
            "expected NotAListForm, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_not_a_list_form_display_matches_legacy() {
        // End-to-end Display rendering: a non-list form fed to
        // `compile_from_sexp` produces the byte-identical legacy
        // string that `tatara-check`, the REPL, and downstream
        // substring-matchers grep on.
        let err = MonitorSpec::compile_from_sexp(&Sexp::int(7)).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: expected list form"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_for_empty_list() {
        // `()` is a list whose first element doesn't exist — head can't
        // be projected to a symbol. The diagnostic names the failure
        // mode AND the structural reason (`(empty list)`) without
        // inventing a "got X" that isn't there. The variant carries
        // `got: None` so an authoring tool can render "your form is
        // empty" without re-parsing the source.
        let err = MonitorSpec::compile_from_sexp(&Sexp::List(vec![])).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    got: None,
                }
            ),
            "expected MissingHeadSymbol {{ got: None }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_for_non_symbol_head() {
        // `(5 :name "x")` — list[0] is `5`, an int, not a symbol. The
        // gate fires AFTER the `as_list` projection succeeds and BEFORE
        // the keyword-equality check; the variant carries `got:
        // Some(SexpWitness { SexpShape::Int, "5" })` so an authoring
        // tool that wants to surface "your form's head is `5`, an int,
        // not a symbol" gains BOTH the typed shape (pattern-matchable)
        // AND the literal value as data, no re-parsing required. The
        // two sub-modes (`()` → `got: None`, `(5 …)` →
        // `got: Some(SexpWitness)`) bind to ONE structural variant —
        // same posture as `RestParamMissingName.got:
        // Option<SexpWitness>`.
        let forms = read(r#"(5 :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    ref got,
                } if got.as_ref().map(|w| (w.shape, w.display.as_str())) == Some((SexpShape::Int, "5"))
            ),
            "expected MissingHeadSymbol {{ got: Some(SexpWitness {{ Int, \"5\" }}) }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_for_keyword_atom_head() {
        // `(:foo :name "x")` — list[0] is the keyword atom `:foo`, not
        // a symbol. The variant's `got` slot carries the typed witness
        // pairing `SexpShape::Keyword` with `Sexp::Display`'s
        // projection of the offending atom (`":foo"`) so the operator
        // sees what they wrote AND tools bind on the typed shape.
        // Pinning across atom kinds (int, keyword) demonstrates that
        // the structural binding is uniform for every non-symbol head.
        let forms = read(r#"(:foo :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    ref got,
                } if got.as_ref().map(|w| (w.shape, w.display.as_str())) == Some((SexpShape::Keyword, ":foo"))
            ),
            "expected MissingHeadSymbol {{ got: Some(SexpWitness {{ Keyword, \":foo\" }}) }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_display_matches_legacy_for_empty_list() {
        // End-to-end Display rendering for the empty-list case: the
        // legacy `Compile { form: "defmonitor", message: "missing head
        // symbol" }` substring (`"compile error in defmonitor:
        // missing head symbol"`) is preserved as the prefix
        // byte-for-byte; the structural detail (`(empty list)`) is
        // appended. Authoring tools (`tatara-check`, the REPL) that
        // substring-grep on the legacy rendering see no drift.
        let err = MonitorSpec::compile_from_sexp(&Sexp::List(vec![])).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (empty list)"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_display_matches_legacy_for_non_symbol_head() {
        // End-to-end Display rendering for the non-symbol-head case:
        // the legacy substring is preserved as the prefix
        // byte-for-byte; the structural detail (`(got 5)`) names
        // the offending head's `Sexp::Display` projection.
        let forms = read(r#"(5 :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (got 5)"
        );
    }

    #[test]
    fn head_mismatch_position_is_none_today() {
        // Negative-control: until `Sexp` carries spans, `position()`
        // returns `None` — `format_diagnostic` falls through to
        // single-line rendering. A future run that adds
        // `pos: Option<usize>` to `HeadMismatch` does so deliberately
        // with a fail-before/pass-after delta.
        let err = head_mismatch("defmonitor", "not-a-monitor".into());
        assert_eq!(err.position(), None);
    }

    // ── suggest — bounded edit-distance over a candidate set ──────────

    #[test]
    fn suggest_picks_single_typo_within_bound() {
        // `tthreshold` differs from `threshold` by one insertion (distance
        // 1). Length 10 → bound 3. The substrate names the likely intended
        // keyword.
        let allowed: &[&str] = &["name", "query", "threshold", "tags", "enabled"];
        assert_eq!(suggest("tthreshold", allowed), Some("threshold"));
    }

    #[test]
    fn suggest_picks_transposition_within_bound() {
        // `htreshold` is one transposition from `threshold` (distance 2 in
        // plain Levenshtein — one delete + one insert). Length 9 → bound 3.
        let allowed: &[&str] = &["name", "query", "threshold"];
        assert_eq!(suggest("htreshold", allowed), Some("threshold"));
    }

    #[test]
    fn suggest_returns_none_when_no_candidate_within_bound() {
        // `garbage` (length 7 → bound 2) is not within distance 2 of any
        // allowed kwarg. The substrate refuses to invent a hint when the
        // distance signal isn't there — a wrong hint is worse than none.
        let allowed: &[&str] = &["name", "query", "threshold", "tags", "enabled"];
        assert_eq!(suggest("garbage", allowed), None);
    }

    #[test]
    fn suggest_excludes_exact_match() {
        // An exact match means the caller already has the keyword; the
        // suggestion exists for near-misses only. Without this guard the
        // primitive would happily echo the input back.
        let allowed: &[&str] = &["name", "query", "threshold"];
        assert_eq!(suggest("name", allowed), None);
    }

    #[test]
    fn suggest_picks_lexicographically_smaller_on_distance_tie() {
        // Two candidates at the same distance — pick the lexicographically
        // smaller one so two operators on two machines see the same hint
        // for the same input. Diagnostics must be deterministic.
        let allowed: &[&str] = &["abc", "abd"]; // both distance 1 from "abe"
        assert_eq!(suggest("abe", allowed), Some("abc"));
    }

    #[test]
    fn suggest_handles_empty_candidates() {
        let allowed: &[&str] = &[];
        assert_eq!(suggest("anything", allowed), None);
    }

    #[test]
    fn suggest_bound_for_short_strings_rejects_distance_two() {
        // Needle length ≤ 3 → bound 1. `abc` vs `xyz` is distance 3 (full
        // replacement); short identifiers are too close to noise to trust
        // a multi-character hint. The bound floor stops false-positives
        // like `:to` matching `:do`.
        let allowed: &[&str] = &["xyz"];
        assert_eq!(suggest("abc", allowed), None);
    }

    #[test]
    fn suggest_bound_for_short_strings_accepts_distance_one() {
        // Within the short-string bound: a single character drift on a
        // 3-character identifier is suggestible.
        let allowed: &[&str] = &["abc"];
        assert_eq!(suggest("abd", allowed), Some("abc"));
    }

    #[test]
    fn suggest_handles_unicode_identifiers() {
        // `levenshtein` operates on chars, not bytes, so a multibyte typo
        // on a multibyte identifier measures character-distance — `é` is
        // one character, not two bytes. Tatara naming is Brazilian ×
        // Japanese (THEORY.md §II.3) so the substrate must not treat
        // non-ASCII as foreign.
        let allowed: &[&str] = &["forjé"];
        assert_eq!(suggest("forje", allowed), Some("forjé"));
    }

    #[test]
    fn reject_unknown_kwargs_includes_did_you_mean_for_near_miss() {
        // End-to-end: a near-miss in the typed-entry gate produces a hint
        // ahead of the allowed-list. The full allowed-list is still in
        // the message — the hint is purely additive.
        let forms = read(r#"(defmonitor :name "x" :tthreshold 0.99)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("did you mean :threshold?"),
            "message must hint the near-match, got: {msg}"
        );
        assert!(
            msg.contains("allowed: "),
            "message must still list the allowed set, got: {msg}"
        );
        assert!(
            msg.contains("unknown keyword"),
            "message must still label the failure, got: {msg}"
        );
    }

    #[test]
    fn reject_unknown_kwargs_omits_did_you_mean_when_no_close_match() {
        // Negative control: when the offending keyword isn't within the
        // edit-distance bound of any allowed kwarg, no hint is fabricated.
        // A wrong hint is worse than no hint.
        let forms = read(r#"(defmonitor :name "x" :totally-unrelated 1)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        let msg = format!("{err}");
        assert!(
            !msg.contains("did you mean"),
            "message must not hint when no close match exists, got: {msg}"
        );
        assert!(
            msg.contains("unknown keyword"),
            "message must still label the failure, got: {msg}"
        );
    }

    #[test]
    fn derive_unknown_keyword_hints_near_miss() {
        // Every derived domain inherits the hint by sharing
        // `reject_unknown_kwargs` — no derive-emit change required.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("did you mean :threshold?"),
            "derived domain must inherit the hint, got: {msg}"
        );
    }

    // ── suggest_keyword — registry-aware near-miss primitive ───────────
    //
    // Wraps `suggest` over `registered_keywords()`. Pinning behavior
    // here covers the substrate-side guarantee every consumer with an
    // unknown registry-dispatched form binds to: ONE primitive, not a
    // per-call-site `registered_keywords()` + `suggest` duplication.

    #[test]
    fn suggest_keyword_picks_near_miss_from_registry() {
        // Register MonitorSpec (idempotent — `register::<T>()` overwrites)
        // so the registry definitely contains `defmonitor` when this
        // test runs, regardless of test ordering.
        register::<MonitorSpec>();
        let hint: Option<&'static str> = suggest_keyword("defmoniter");
        assert_eq!(
            hint,
            Some("defmonitor"),
            "registry-aware near-miss must resolve `defmoniter` to `defmonitor`"
        );
    }

    #[test]
    fn suggest_keyword_excludes_exact_match() {
        // When the needle IS a registered keyword, no hint — the
        // suggestion is for near-misses only. Same posture `suggest`
        // takes for general candidate sets.
        register::<MonitorSpec>();
        assert_eq!(
            suggest_keyword("defmonitor"),
            None,
            "exact registry hits must not echo as suggestions"
        );
    }

    #[test]
    fn suggest_keyword_returns_none_when_no_close_match() {
        // Needle far enough from any plausible domain keyword that no
        // registered keyword (now or in the future) lands within the
        // bounded edit distance — no false-positive hint.
        register::<MonitorSpec>();
        assert_eq!(
            suggest_keyword("xyzqrstuvwx"),
            None,
            "needle outside the bound must not produce a hint"
        );
    }

    // ── unknown_domain_keyword — structural variant + named primitive ─
    //
    // Pairs `LispError::UnknownDomainKeyword { keyword, hint, registered }`
    // with `unknown_domain_keyword(keyword)` so the registry-dispatch
    // fallthrough (`tatara-check`'s unknown `(defX …)` path) binds to ONE
    // primitive instead of inline `format!("did you mean ({m} ...)? ")` +
    // `format!("Registered domains: {:?}", registered_keywords())` +
    // `report.fail(label, detail)` triples. The shape mirrors
    // `unknown_kwarg`: same three slots (offending key + optional hint +
    // sorted candidate set), same deterministic-ordering posture, same
    // owned-data lifetime contract — the substrate's unknown-something-
    // against-a-set diagnostic surface is now a single shape.
    //
    // Tests pin: variant identity, hint resolution, hint absence, sorted
    // determinism, kebab-case round-trip, end-to-end Display.

    #[test]
    fn unknown_domain_keyword_emits_structural_variant_with_hint() {
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("defmoniter");
        match err {
            LispError::UnknownDomainKeyword {
                keyword,
                hint,
                registered,
            } => {
                assert_eq!(keyword, "defmoniter");
                assert_eq!(hint.as_deref(), Some("defmonitor"));
                assert!(
                    registered.contains(&"defmonitor".to_string()),
                    "registered set must include the registered keyword(s); got {registered:?}"
                );
            }
            other => panic!("expected UnknownDomainKeyword, got {other:?}"),
        }
    }

    #[test]
    fn unknown_domain_keyword_emits_structural_variant_without_hint_when_no_close_match() {
        // Needle far from any registered keyword — the hint slot stays
        // empty (a wrong hint is worse than no hint). This is the
        // structural counterpart to `suggest_keyword_returns_none_when_no_close_match`
        // — `unknown_domain_keyword` carries the absence into the variant.
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("xyzqrstuvwx");
        match err {
            LispError::UnknownDomainKeyword {
                keyword,
                hint,
                registered,
            } => {
                assert_eq!(keyword, "xyzqrstuvwx");
                assert!(
                    hint.is_none(),
                    "needle outside the bound must produce no hint"
                );
                assert!(!registered.is_empty());
            }
            other => panic!("expected UnknownDomainKeyword, got {other:?}"),
        }
    }

    #[test]
    fn unknown_domain_keyword_sorts_registered_set_lexicographically() {
        // Registry iteration order is HashMap-derived (non-deterministic),
        // so the helper sorts the registered set before placing it in the
        // variant. A regression that drops the sort and lets HashMap
        // iteration order leak into the diagnostic fails-loudly here.
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("totally-unrelated-form");
        match err {
            LispError::UnknownDomainKeyword { registered, .. } => {
                let mut expected = registered.clone();
                expected.sort();
                assert_eq!(
                    registered, expected,
                    "registered keyword set must be sorted lexicographically"
                );
            }
            other => panic!("expected UnknownDomainKeyword, got {other:?}"),
        }
    }

    #[test]
    fn unknown_domain_keyword_display_matches_structural_shape_with_hint() {
        // End-to-end Display from the helper: the offending head's call
        // shape, the structural near-miss in the same call shape, and
        // the registered set. The shape is byte-stable so authoring
        // surfaces that substring-match on the rendered diagnostic see
        // no drift across registry mutations (modulo the registered
        // set itself).
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("defmoniter");
        let rendered = format!("{err}");
        assert!(
            rendered.starts_with("unknown domain keyword: (defmoniter ...)"),
            "rendered diagnostic must lead with the offending head: {rendered}"
        );
        assert!(
            rendered.contains("did you mean (defmonitor ...)?"),
            "rendered diagnostic must surface the structural near-miss: {rendered}"
        );
        assert!(
            rendered.contains("registered: "),
            "rendered diagnostic must include the registered set: {rendered}"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_carries_kebab_case_keywords_unchanged() {
        // Kebab-cased domain keywords (a future `defalert-policy`,
        // `defprocess-spec`) round-trip through the offending-keyword
        // slot AND the registered-list slot unchanged. The substrate's
        // diagnostic surface respects the author's casing.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defalert-policiy".into(),
            hint: Some("defalert-policy".into()),
            registered: vec!["defalert-policy".into(), "defprocess-spec".into()],
        };
        assert!(format!("{err}").contains("(defalert-policiy ...)"));
        assert!(format!("{err}").contains("(defalert-policy ...)?"));
        assert!(format!("{err}").contains("registered: defalert-policy, defprocess-spec"));
    }

    // ── Structural DuplicateKwarg variant ─────────────────────────────
    //
    // `parse_kwargs`'s top-level duplicate path and `sexp_to_json`'s
    // nested-kwargs duplicate path used to emit identical inline triples:
    //   `LispError::Compile { form: kwarg_form(k), message: "duplicate
    //    keyword".into() }`.
    // Two copies in one module is the prime-directive precursor to the
    // three-times rule (THEORY.md §VI.1) — and the diagnostic *category*
    // ("a kwargs slice contained `:k` twice") is structurally distinct
    // from every other typed-entry mismatch shape, so it deserves its
    // own structural variant the same way `OddKwargs` does.
    //
    // After this lift `parse_kwargs`'s diagnostic surface is structurally
    // complete — every distinct failure mode binds to ONE structural
    // variant of `LispError`:
    //   * odd length        → `LispError::OddKwargs { dangling }`
    //   * not-a-keyword-pos → `LispError::TypeMismatch { form, … }`
    //   * duplicate key     → `LispError::DuplicateKwarg { key }`
    // No `parse_kwargs` failure produces an unstructured `Compile` shape.
    //
    // Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
    // so existing `msg.contains("duplicate keyword")` /
    // `msg.contains(":name")` assertions pass; the gain is structural —
    // authoring surfaces (REPL, LSP, `tatara-check`) bind to the variant.

    #[test]
    fn duplicate_kwarg_emits_structural_variant() {
        let err = duplicate_kwarg("name");
        match err {
            LispError::DuplicateKwarg { key } => assert_eq!(key, "name"),
            other => panic!("expected DuplicateKwarg, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_kwarg_display_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { form: ":name", message:
        // "duplicate keyword" }` rendering. Authoring surfaces that
        // pattern-match on the message text continue to work; tools that
        // pattern-match on the variant gain structural binding.
        let err = duplicate_kwarg("threshold");
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: duplicate keyword"
        );
    }

    #[test]
    fn duplicate_kwarg_preserves_kebab_case_keys() {
        // Multi-segment kebab-cased keys (`:notify-ref`, `:window-seconds`)
        // ride through unchanged. A regression that camelCases or
        // lowercases the key in the rendered diagnostic fails-loudly.
        let err = duplicate_kwarg("notify-ref");
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: duplicate keyword"
        );
    }

    #[test]
    fn parse_kwargs_top_level_duplicate_emits_structural_variant() {
        // `(_ :name "x" :name "y")` — top-level duplicate. Replaces the
        // legacy `Compile { form: ":name", message: "duplicate keyword" }`
        // shape with the structural `DuplicateKwarg { key: "name" }`.
        let args = kwargs_of(r#"(_ :name "x" :name "y")"#);
        let err = parse_kwargs(&args).unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "name"),
            "expected DuplicateKwarg {{ key: \"name\" }}, got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_duplicate_message_renders_canonical_shape() {
        // Pin the rendered Display shape so authoring tools that already
        // substring-match `duplicate keyword` (and `tatara-check`'s
        // user-defined `defcheck` macros) light up uniformly. A
        // regression that drifts the separator (e.g. `kwargs.name`) or
        // the label (e.g. `repeated key`) fails-loudly here.
        let args = kwargs_of(r#"(_ :threshold 0.1 :threshold 0.2)"#);
        let err = parse_kwargs(&args).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: duplicate keyword"
        );
    }

    #[test]
    fn sexp_to_json_nested_duplicate_emits_structural_variant() {
        // `:step (:notify-ref "a" :notify-ref "b")` — the duplicate fires
        // during the `sexp_to_json` projection, before serde sees a
        // value. The lift gives the nested path the SAME structural
        // variant as the top-level path; the operator sees one shape
        // regardless of which depth misfired.
        let args = kwargs_of(r#"(_ :step (:notify-ref "a" :notify-ref "b"))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<EscalationStep>(&kw, "step").unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "notify-ref"),
            "expected DuplicateKwarg {{ key: \"notify-ref\" }} from nested kwargs, got {err:?}"
        );
    }

    #[test]
    fn extract_vec_via_serde_inner_duplicate_emits_structural_variant() {
        // `:steps ((:notify-ref "a" :notify-ref "b"))` — the duplicate is
        // inside one vec item. The `sexp_to_json` path fires before the
        // per-item serde wrapper sees a value, so the inner
        // `DuplicateKwarg` variant propagates with the inner kwarg's key
        // — not clobbered by `:steps[0]`. Pinning this means the
        // operator can pattern-match on `key == "notify-ref"` regardless
        // of vec nesting.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "notify-ref"),
            "expected DuplicateKwarg {{ key: \"notify-ref\" }} from vec-item kwargs, got {err:?}"
        );
    }

    #[test]
    fn derive_duplicate_kwarg_e2e_emits_structural_variant() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // every derived domain inherits the structural variant by
        // sharing `parse_kwargs`. No per-derive macro change is
        // required.
        let forms = read(r#"(defmonitor :name "x" :name "y" :query "q" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "name"),
            "derived domain must surface DuplicateKwarg, got {err:?}"
        );
    }

    #[test]
    fn duplicate_kwarg_position_is_none_today() {
        // Negative-control: until `Sexp` carries spans, `position()`
        // returns `None` for the variant — `format_diagnostic` falls
        // through to single-line rendering, no caret emitted. Pinning
        // this contract means a future run that adds `pos: Option<usize>`
        // does so deliberately, with a fail-before/pass-after delta.
        let err = duplicate_kwarg("name");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn suggest_keyword_result_is_static_str() {
        // The substrate hands back the SAME `&'static str` the registry
        // stores — every registered keyword is `'static` (the trait's
        // `KEYWORD` const), so `suggest_keyword` borrows from `'static`,
        // not from a temporary `Vec`. Pinning the lifetime here keeps
        // future consumers (LSP / REPL / forge) safe to embed the hint
        // in a `&'static str`-typed slot without an allocation.
        register::<MonitorSpec>();
        let hint: Option<&'static str> = suggest_keyword("defmoniter");
        // Force the result through a `'static`-bound slot — if the
        // signature ever drops `'static`, this fails to compile, which
        // is exactly the safety net we want.
        fn requires_static(_s: &'static str) {}
        if let Some(s) = hint {
            requires_static(s);
        }
        assert!(hint.is_some());
    }

    // ── Structural MissingKwarg variant ───────────────────────────────
    //
    // `required` is the kwarg-lookup helper that fronts every typed
    // extractor (`extract_string`, `extract_int`, `extract_float`,
    // `extract_bool`, `extract_via_serde`) and every hand-written
    // `TataraDomain` impl that needs a kwarg-by-runtime-key. It used to
    // assemble the "required but absent" diagnostic inline:
    //   `LispError::Compile { form: kwarg_form(key), message: "required
    //    but not provided".into() }`.
    // The diagnostic *category* ("a required kwarg :k was not provided")
    // is structurally distinct from every other typed-entry mismatch —
    // it has no `expected/got` axis, no item index, no near-miss hint —
    // so it deserves its own structural variant the same way `OddKwargs`
    // and `DuplicateKwarg` do.
    //
    // After this lift `parse_kwargs` + `required` cover every
    // typed-entry kwarg failure mode with a structural variant of
    // `LispError`:
    //   * odd length        → `LispError::OddKwargs { dangling }`
    //   * not-a-keyword-pos → `LispError::TypeMismatch { form, … }`
    //   * duplicate key     → `LispError::DuplicateKwarg { key }`
    //   * missing required  → `LispError::MissingKwarg { key }`
    // No kwarg-lookup failure produces an unstructured `Compile` shape.
    //
    // `MissingKwarg` is the runtime-key sibling of the pre-existing
    // `Missing(&'static str)` variant — `Missing` stays for compile-
    // time-known names; `MissingKwarg` covers the runtime-key path
    // every kwargs extractor shares.
    //
    // Display matches the legacy `Compile`-shaped diagnostic byte-for-
    // byte so existing `msg.contains("required")` /
    // `msg.contains(":threshold")` assertions pass unchanged; the gain
    // is structural — authoring surfaces (REPL, LSP, `tatara-check`)
    // bind to the variant.

    #[test]
    fn missing_kwarg_emits_structural_variant() {
        let err = missing_kwarg("name");
        match err {
            LispError::MissingKwarg { key } => assert_eq!(key, "name"),
            other => panic!("expected MissingKwarg, got {other:?}"),
        }
    }

    #[test]
    fn missing_kwarg_display_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { form: ":threshold", message:
        // "required but not provided" }` rendering. Authoring surfaces
        // that pattern-match on the message text continue to work; tools
        // that pattern-match on the variant gain structural binding.
        let err = missing_kwarg("threshold");
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: required but not provided"
        );
    }

    #[test]
    fn missing_kwarg_preserves_kebab_case_keys() {
        // Multi-segment kebab-cased keys (`:notify-ref`, `:window-seconds`)
        // ride through unchanged. A regression that camelCases or
        // lowercases the key in the rendered diagnostic fails-loudly.
        let err = missing_kwarg("notify-ref");
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: required but not provided"
        );
    }

    #[test]
    fn required_emits_structural_variant_when_absent() {
        // `(_ :other 1)` looking up `:level` — the kwarg is not in the
        // map. `required` must surface the structural `MissingKwarg`,
        // not the legacy `Compile`. Pin the variant identity AND the
        // key so a regression that re-inlines the inline shape fails-
        // loudly here.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = required(&kw, "level").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "level"),
            "expected MissingKwarg {{ key: \"level\" }}, got {err:?}"
        );
    }

    #[test]
    fn required_present_kwarg_returns_value_unchanged() {
        // Negative-control: when the kwarg IS present, `required`
        // returns its value — the structural-variant lift is for the
        // absent-key path only.
        let args = kwargs_of(r#"(_ :level "info")"#);
        let kw = parse_kwargs(&args).unwrap();
        let v = required(&kw, "level").expect("present kwarg must return Ok");
        assert_eq!(v.as_string(), Some("info"));
    }

    #[test]
    fn required_message_renders_canonical_shape() {
        // Pin the rendered Display shape so authoring tools that already
        // substring-match `required` (and `tatara-check`'s
        // user-defined `defcheck` macros) light up uniformly. A
        // regression that drifts the separator or the label fails-
        // loudly here.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = required(&kw, "threshold").unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: required but not provided"
        );
    }

    // ── optional: the may-be-absent kwargs-lookup primitive ──────────────
    //
    // `optional(kw, key)` is the typed-entry kwargs-gate's sibling of
    // `required`: present → `Some(&Sexp)`, absent → `None`. Before this
    // lift the same `kw.get(key).copied()` projection was inlined at
    // FOUR sites — `required` (composed atop `optional + ok_or_else`),
    // `extract_optional_atom` (absence → `Ok(None)`), `extract_list`
    // (absence → `Ok(Vec::new())`), and `extract_optional_via_serde`
    // (absence → `Ok(None)`). The five tests below pin: (a) absent →
    // `None`; (b) present → `Some(&Sexp)` with value equality; (c) the
    // returned `&Sexp` borrows from the kwargs map (lifetime contract);
    // (d) the sibling composition `required = optional + ok_or_else`
    // is structurally observable; (e) the three optional-path
    // extractor consumers route through it (path-uniformity across
    // the lift). Together they pin the named PAIR `{required,
    // optional}` as the substrate's typed-entry kwargs-lookup
    // surface.

    #[test]
    fn optional_returns_none_when_key_absent() {
        // Negative-control: the kwarg is not in the map, so `optional`
        // surfaces `None` — the consumer's absence-arm input. No
        // diagnostic, no allocation, no `Result` indirection.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert!(optional(&kw, "level").is_none());
    }

    #[test]
    fn optional_returns_some_value_when_key_present() {
        // Positive-control: the kwarg IS in the map, so `optional`
        // surfaces `Some(&Sexp)` with the bound value. The returned
        // reference carries the same value `parse_kwargs` parked at
        // the key — no copying, no normalization.
        let args = kwargs_of(r#"(_ :level "info")"#);
        let kw = parse_kwargs(&args).unwrap();
        let v = optional(&kw, "level").expect("present kwarg must return Some");
        assert_eq!(v.as_string(), Some("info"));
    }

    #[test]
    fn optional_borrow_lifetime_outlives_map_lookup() {
        // The returned `&'a Sexp` borrows from the kwargs map's value
        // slot via `.copied()`, so a consumer can hold the reference
        // through its absence-arm match without an intermediate
        // clone — same lifetime contract as `required`'s `Ok(&Sexp)`
        // return. Pin it by reading the value AFTER the
        // `Option::expect`, against a freshly-bound reference whose
        // lifetime is tied to the outer `kw` binding.
        let args = kwargs_of(r#"(_ :name "obs" :threshold 0.99)"#);
        let kw = parse_kwargs(&args).unwrap();
        let name_ref: &Sexp = optional(&kw, "name").expect("present");
        let thr_ref: &Sexp = optional(&kw, "threshold").expect("present");
        assert_eq!(name_ref.as_string(), Some("obs"));
        assert_eq!(thr_ref.as_float(), Some(0.99));
    }

    #[test]
    fn required_is_optional_composed_with_missing_kwarg() {
        // The sibling composition `required = optional +
        // ok_or_else(missing_kwarg)` is structurally observable: on
        // the absent path, `required(kw, key).unwrap_err()` and
        // `optional(kw, key).ok_or_else(|| missing_kwarg(key))
        // .unwrap_err()` must produce structurally-equal errors;
        // on the present path, both projections name the SAME `&Sexp`
        // pointer (via `Result::ok` / `Option`). Pin both directions.
        let args = kwargs_of(r#"(_ :name "obs")"#);
        let kw = parse_kwargs(&args).unwrap();

        // Present-path identity: required returns the same &Sexp the
        // optional lookup found.
        let via_required = required(&kw, "name").expect("present");
        let via_optional = optional(&kw, "name").expect("present");
        assert!(
            std::ptr::eq(via_required, via_optional),
            "required and optional must surface the SAME &Sexp pointer for a present kwarg"
        );

        // Absent-path identity: required's error matches the closed-
        // form composition's error.
        let err_required = required(&kw, "absent").unwrap_err();
        let err_composed = optional(&kw, "absent")
            .ok_or_else(|| missing_kwarg("absent"))
            .unwrap_err();
        assert_eq!(format!("{err_required}"), format!("{err_composed}"));
        assert!(matches!(
            err_required,
            LispError::MissingKwarg { ref key } if key == "absent"
        ));
    }

    #[test]
    fn extract_optional_atom_routes_through_optional() {
        // Path-uniformity: `extract_optional_string` (which fronts
        // `extract_optional_atom`) now reads the kwarg through
        // `optional`. Absent → `Ok(None)` (no rejection); present →
        // `Ok(Some(value))`. Pin both arms so a regression that
        // re-inlines the `kw.get(key).copied()` projection at the
        // call site fails loudly here.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_string(&absent_kw, "name").unwrap(),
            None,
            "absent optional kwarg must surface as Ok(None)"
        );

        let present_args = kwargs_of(r#"(_ :name "obs")"#);
        let present_kw = parse_kwargs(&present_args).unwrap();
        assert_eq!(
            extract_optional_string(&present_kw, "name").unwrap(),
            Some("obs"),
            "present optional kwarg must surface as Ok(Some(value))"
        );
    }

    #[test]
    fn extract_list_routes_through_optional_on_absent_key() {
        // Path-uniformity: `extract_string_list` (which fronts
        // `extract_list`) now reads the kwarg through `optional`.
        // Absent → `Ok(Vec::new())` (the empty-list absence floor —
        // never an error, parallel to `extract_optional_atom`'s
        // `Ok(None)`).
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_string_list(&kw, "tags").unwrap(),
            Vec::<String>::new(),
            "absent list kwarg must surface as Ok(Vec::new())"
        );
    }

    #[test]
    fn extract_optional_via_serde_routes_through_optional() {
        // Path-uniformity: `extract_optional_via_serde` now reads the
        // kwarg through `optional`. Absent → `Ok(None)`; present →
        // `Ok(Some(value))` after the canonical-JSON round-trip.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        let absent: Option<i64> = extract_optional_via_serde(&absent_kw, "n").unwrap();
        assert_eq!(absent, None, "absent serde-fallthrough kwarg → Ok(None)");

        let present_args = kwargs_of("(_ :n 42)");
        let present_kw = parse_kwargs(&present_args).unwrap();
        let present: Option<i64> = extract_optional_via_serde(&present_kw, "n").unwrap();
        assert_eq!(
            present,
            Some(42),
            "present serde-fallthrough kwarg → Ok(Some(value))"
        );
    }

    #[test]
    fn extract_string_missing_emits_structural_variant() {
        // `extract_string` fronts every other typed extractor by
        // routing through `required`. A missing kwarg must produce
        // `MissingKwarg`, not `TypeMismatch` (no value to type-check)
        // and not `Compile` (legacy shape). Pin the routing.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string(&kw, "name").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "name"),
            "expected MissingKwarg from extract_string, got {err:?}"
        );
    }

    #[test]
    fn extract_int_missing_emits_structural_variant() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_int(&kw, "n").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "n"),
            "expected MissingKwarg from extract_int, got {err:?}"
        );
    }

    #[test]
    fn extract_float_missing_emits_structural_variant() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_float(&kw, "ratio").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "ratio"),
            "expected MissingKwarg from extract_float, got {err:?}"
        );
    }

    #[test]
    fn extract_bool_missing_emits_structural_variant() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_bool(&kw, "enabled").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "enabled"),
            "expected MissingKwarg from extract_bool, got {err:?}"
        );
    }

    #[test]
    fn extract_via_serde_missing_emits_structural_variant() {
        // The serde-fallthrough path also routes through `required`, so
        // every typed `Deserialize` field (enums, nested structs, vecs
        // of nested structs) inherits the structural variant for the
        // absent-key case — uniform shape across the typed-extractor
        // and the serde-fallthrough surfaces.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "level"),
            "expected MissingKwarg from extract_via_serde, got {err:?}"
        );
    }

    #[test]
    fn derive_missing_required_kwarg_e2e_emits_structural_variant() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // omitting the required `:threshold` must surface the structural
        // `MissingKwarg { key: "threshold" }` — every derived domain
        // inherits the lift by sharing `required`. No per-derive macro
        // change required.
        let forms = read(r#"(defmonitor :name "x" :query "q")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "threshold"),
            "derived domain must surface MissingKwarg, got {err:?}"
        );
    }

    #[test]
    fn missing_kwarg_position_is_none_today() {
        // Negative-control for the future-spans move: until `Sexp`
        // carries source positions, the variant's `position()` returns
        // `None`. Pinning this contract means a future run that adds
        // `pos: Option<usize>` to `MissingKwarg` does so deliberately —
        // the missing-kwarg path picks up the span automatically because
        // it routes through the same primitive (`missing_kwarg`) as
        // every other call site.
        let err = missing_kwarg("name");
        assert_eq!(err.position(), None);
    }

    // ── Structural UnknownKwarg variant ───────────────────────────────
    //
    // `reject_unknown_kwargs` used to assemble its diagnostic inline:
    //   `LispError::Compile { form: kwarg_form(key), message: format!(
    //       "unknown keyword (did you mean :{hint}?; allowed: ...)"
    //    ) }`
    // — the offending key, the near-miss hint, and the allowed-set
    // were all welded into a free-form `message` string. After this
    // lift the three slots are first-class fields on
    // `LispError::UnknownKwarg { key, hint, allowed }`, so authoring
    // surfaces (REPL, LSP, `tatara-check`) bind to the variant
    // structurally instead of substring-parsing the rendered message.
    //
    // This is the FIFTH and LAST structural-variant lift on the
    // typed-entry kwarg-gate's diagnostic surface — every distinct
    // failure mode is now a structural variant of `LispError`:
    //   * odd length        → `LispError::OddKwargs { dangling }`
    //   * not-a-keyword-pos → `LispError::TypeMismatch { form, … }`
    //   * duplicate key     → `LispError::DuplicateKwarg { key }`
    //   * missing required  → `LispError::MissingKwarg { key }`
    //   * unknown keyword   → `LispError::UnknownKwarg { key, hint,
    //                                                   allowed }`
    // No kwarg-gate failure produces an unstructured `Compile` shape.
    //
    // Display matches the legacy `Compile`-shaped diagnostic byte-
    // for-byte so existing `msg.contains("unknown keyword")` /
    // `msg.contains(":threshold")` / `msg.contains("did you mean
    // :threshold?")` / `msg.contains("allowed: ")` assertions pass;
    // the gain is structural — authoring surfaces bind to the variant.

    #[test]
    fn unknown_kwarg_emits_structural_variant_with_hint() {
        // `tthreshold` is a near-miss of `threshold`; `suggest` ranks
        // it within the bounded edit distance, so `unknown_kwarg`
        // populates the `hint` slot with the allowed candidate.
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("tthreshold", allowed);
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
                // `unknown_kwarg` sorts the allowed set lexicographically
                // so two operators on two machines see the same
                // diagnostic for the same input — diagnostics are
                // deterministic regardless of HashMap iteration order.
                assert_eq!(alw, vec!["name", "query", "threshold"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_emits_structural_variant_without_hint_when_no_close_match() {
        // Negative control: when the offending keyword isn't within the
        // edit-distance bound of any allowed kwarg, no hint is
        // fabricated. A wrong hint is worse than no hint.
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("totally-unrelated", allowed);
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "totally-unrelated");
                assert!(hint.is_none(), "no near-miss must produce no hint");
                assert_eq!(alw, vec!["name", "query", "threshold"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_sorts_allowed_set_lexicographically() {
        // `unknown_kwarg` is the single named primitive that materializes
        // the allowed-set as owned `Vec<String>` and sorts it
        // lexicographically. Pin the sort so a regression that drops it
        // (and thus drifts the rendered message order across HashMap
        // iteration ordering) fails-loudly here.
        let allowed: &[&str] = &["zeta", "alpha", "mu", "beta"];
        let err = unknown_kwarg("xx", allowed);
        match err {
            LispError::UnknownKwarg { allowed: alw, .. } => {
                assert_eq!(alw, vec!["alpha", "beta", "mu", "zeta"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_display_with_hint_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { form: ":tthreshold", message:
        // "unknown keyword (did you mean :threshold?; allowed: :name,
        // :query, :threshold)" }` rendering. Authoring surfaces that
        // pattern-match on the message text continue to work; tools
        // that pattern-match on the variant gain structural binding.
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("tthreshold", allowed);
        assert_eq!(
            format!("{err}"),
            "compile error in :tthreshold: unknown keyword \
             (did you mean :threshold?; allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_display_without_hint_matches_legacy_compile_shape() {
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("totally-unrelated", allowed);
        assert_eq!(
            format!("{err}"),
            "compile error in :totally-unrelated: unknown keyword \
             (allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_preserves_kebab_case_keys() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg
        // name round-trips through both the offending-key slot AND the
        // allowed-list slot unchanged. A regression that camelCases or
        // lowercases either side fails-loudly here.
        let allowed: &[&str] = &["notify-ref", "window-seconds"];
        let err = unknown_kwarg("windou-seconds", allowed);
        assert_eq!(
            format!("{err}"),
            "compile error in :windou-seconds: unknown keyword \
             (did you mean :window-seconds?; allowed: :notify-ref, :window-seconds)"
        );
    }

    #[test]
    fn reject_unknown_kwargs_emits_structural_variant_for_typo() {
        // End-to-end: `reject_unknown_kwargs` must surface the
        // structural `UnknownKwarg`, not the legacy `Compile`. Pin the
        // variant identity AND the key so a regression that re-inlines
        // the inline shape fails-loudly here.
        let forms = read(r#"(defmonitor :name "x" :tthreshold 0.99)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
                assert!(
                    alw.contains(&"threshold".to_string()),
                    "allowed-set must include `threshold`, got {alw:?}"
                );
                assert_eq!(
                    alw,
                    vec![
                        "enabled",
                        "name",
                        "query",
                        "tags",
                        "threshold",
                        "window-seconds"
                    ],
                    "allowed-set must be lexicographically sorted"
                );
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn reject_unknown_kwargs_passes_when_all_known_returns_ok() {
        // Negative control: when every kwarg IS in the allowed set,
        // `reject_unknown_kwargs` returns `Ok(())` — the structural-
        // variant lift is for the unknown path only.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &["name", "query", "threshold"];
        assert!(reject_unknown_kwargs(&kw, allowed).is_ok());
    }

    #[test]
    fn derive_unknown_kwarg_e2e_emits_structural_variant() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // a typo'd `:tthreshold` must surface the structural
        // `UnknownKwarg { key: "tthreshold", hint: Some("threshold"),
        // allowed }` — every derived domain inherits the lift by
        // sharing `reject_unknown_kwargs`. No per-derive macro change
        // required.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        match err {
            LispError::UnknownKwarg { key, hint, .. } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
            }
            other => panic!("derived domain must surface UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_position_is_none_today() {
        // Negative-control for the future-spans move: until `Sexp`
        // carries source positions, the variant's `position()` returns
        // `None`. Pinning this contract means a future run that adds
        // `pos: Option<usize>` to `UnknownKwarg` does so deliberately —
        // the unknown-kwarg path picks up the span automatically
        // because it routes through the same primitive
        // (`unknown_kwarg`) as every other call site.
        let allowed: &[&str] = &["name"];
        let err = unknown_kwarg("xx", allowed);
        assert_eq!(err.position(), None);
    }

    // ── parse_kwargs_strict: the fused typed-entry kwargs gate ──────────
    //
    // `parse_kwargs_strict(args, allowed)` is the substrate-level
    // composition of `parse_kwargs` + `reject_unknown_kwargs`, the
    // two-call sequence every `#[derive(TataraDomain)]`-generated
    // `compile_from_args` emits at its header and every hand-written
    // impl in the forge / lattice / tameshi crates inlines verbatim.
    // After this lift the fleet's seven-plus consumers (and every
    // future derived domain) route through ONE function the substrate
    // owns, instead of two functions every consumer must remember to
    // call in the canonical parse-then-reject order.
    //
    // The tests below pin the fused primitive's contract: (a) on
    // well-formed input it produces the same `Kwargs<'_>` map as the
    // two-step inlined call; (b)-(d) every parse-stage failure mode
    // surfaces as the same structural variant `parse_kwargs` would
    // raise (`OddKwargs` / `DuplicateKwarg` / `TypeMismatch`);
    // (e) every reject-stage failure surfaces as the same `UnknownKwarg`
    // `reject_unknown_kwargs` would raise; (f)-(g) parse-stage rejection
    // STRICTLY precedes reject-stage rejection — calls that violate
    // BOTH stages surface the parse-stage variant, never the reject-
    // stage variant; (h) an empty allowed-set rejects every parsed
    // kwarg as unknown (negative control on the closed-set posture);
    // (i) end-to-end through `MonitorSpec::compile_from_args` — the
    // derive's emit now routes through `parse_kwargs_strict`, so a
    // derived domain's diagnostics inherit the fused primitive's
    // single-call-site identity.

    #[test]
    fn parse_kwargs_strict_well_formed_input_matches_two_step_path() {
        // Path-uniformity: on a well-formed kwargs run with every key
        // in the allowed set, `parse_kwargs_strict` returns the same
        // map `parse_kwargs` would, with `reject_unknown_kwargs` having
        // returned `Ok(())` against it. The fused primitive is the
        // substrate-level composition of the two stages; on the
        // happy path the composition is observationally identical to
        // the two-step inlined call.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("x"),
            Sexp::keyword("query"),
            Sexp::string("q"),
            Sexp::keyword("threshold"),
            Sexp::float(0.5),
        ];
        let allowed: &[&str] = &["name", "query", "threshold"];

        let fused = parse_kwargs_strict(&args, allowed).expect("well-formed must parse strictly");
        let staged = parse_kwargs(&args).expect("well-formed must parse");
        assert!(reject_unknown_kwargs(&staged, allowed).is_ok());

        // The fused map has the same keys + structurally-equal values
        // as the two-step map. (We compare via the sorted key list +
        // per-key Sexp equality because `&Sexp` borrows from `args` on
        // both sides — same lifetime, same source slice.)
        let mut fused_keys: Vec<&str> = fused.keys().map(String::as_str).collect();
        let mut staged_keys: Vec<&str> = staged.keys().map(String::as_str).collect();
        fused_keys.sort();
        staged_keys.sort();
        assert_eq!(fused_keys, staged_keys);
        for k in fused_keys {
            assert_eq!(fused.get(k), staged.get(k));
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_odd_length_to_parse_stage_variant() {
        // Parse-stage rejection: an odd-length kwargs tail must surface
        // as `LispError::OddKwargs` — the same structural variant
        // `parse_kwargs` would raise. The reject-stage never runs
        // because the parse stage short-circuits on `Err`.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("x"),
            Sexp::keyword("query"),
        ];
        let allowed: &[&str] = &["name", "query"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("odd-length args must reject at parse stage");
        match err {
            LispError::OddKwargs { dangling } => {
                assert_eq!(dangling, ":query");
            }
            other => panic!("expected OddKwargs, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_duplicate_key_to_parse_stage_variant() {
        // Parse-stage rejection: a repeated `:name` key must surface as
        // `LispError::DuplicateKwarg` — same posture as `parse_kwargs`.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("a"),
            Sexp::keyword("name"),
            Sexp::string("b"),
        ];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("duplicate-key args must reject at parse stage");
        match err {
            LispError::DuplicateKwarg { key } => {
                assert_eq!(key, "name");
            }
            other => panic!("expected DuplicateKwarg, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_non_keyword_position_to_type_mismatch_variant() {
        // Parse-stage rejection: an integer where a keyword was expected
        // (position 0) must surface as `LispError::TypeMismatch` with
        // `form = kwargs_pos_form(0)` and `expected = Keyword` — same
        // posture as `parse_kwargs`'s direct slot-must-be-a-keyword
        // rejection.
        let args = [Sexp::int(5), Sexp::string("x")];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("non-keyword at key position must reject at parse stage");
        match err {
            LispError::TypeMismatch { expected, got, .. } => {
                assert_eq!(expected, ExpectedKwargShape::Keyword);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_unknown_kwarg_to_reject_stage_variant() {
        // Reject-stage rejection: a well-formed parse with a key
        // OUTSIDE the allowed set surfaces as `LispError::UnknownKwarg`
        // with the typed `hint` / `allowed` slots — same posture as
        // `reject_unknown_kwargs`.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("x"),
            Sexp::keyword("tthreshold"),
            Sexp::float(0.99),
        ];
        let allowed: &[&str] = &["name", "threshold"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("unknown kwarg must reject at reject stage");
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
                assert_eq!(alw, vec!["name", "threshold"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_parse_stage_fires_before_reject_stage_on_odd_length() {
        // Stage-ordering: a call whose tail is BOTH odd-length AND
        // contains an unknown kwarg surfaces as `OddKwargs` (parse
        // stage), NOT `UnknownKwarg` (reject stage). The fused
        // primitive's composition order is load-bearing: parse runs
        // first, reject runs second, and the second stage cannot
        // observe an `Err` from the first.
        let args = [
            Sexp::keyword("ghost"),
            Sexp::string("boo"),
            Sexp::keyword("orphan"),
        ];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("odd-length + unknown must reject at parse stage");
        match err {
            LispError::OddKwargs { dangling } => {
                assert_eq!(dangling, ":orphan");
            }
            other => panic!("expected OddKwargs (parse stage fires first), got {other:?}",),
        }
    }

    #[test]
    fn parse_kwargs_strict_parse_stage_fires_before_reject_stage_on_duplicate() {
        // Stage-ordering, sibling case: a duplicate-key kwargs tail with
        // an extra unknown key still surfaces as `DuplicateKwarg`
        // (parse stage). The parse-stage walk reaches the duplicate
        // BEFORE the reject stage ever inspects the keyset.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("a"),
            Sexp::keyword("ghost"),
            Sexp::string("boo"),
            Sexp::keyword("name"),
            Sexp::string("b"),
        ];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("duplicate + unknown must reject at parse stage");
        match err {
            LispError::DuplicateKwarg { key } => {
                assert_eq!(key, "name");
            }
            other => panic!("expected DuplicateKwarg (parse stage fires first), got {other:?}",),
        }
    }

    #[test]
    fn parse_kwargs_strict_empty_allowed_set_rejects_every_parsed_kwarg() {
        // Closed-set posture floor: an empty `allowed: &[]` means the
        // domain admits NO kwargs at all. Any well-formed kwarg
        // parses successfully but the reject stage rejects the first
        // key it sees as `UnknownKwarg`. The allowed-set lives at
        // ONE call site (`parse_kwargs_strict`), so a future "domain
        // with no kwargs" emits `parse_kwargs_strict(args, &[])` and
        // inherits the rejection posture without re-deriving it.
        let args = [Sexp::keyword("name"), Sexp::string("x")];
        let allowed: &[&str] = &[];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("empty allowed-set must reject any parsed kwarg");
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "name");
                // No allowed candidates → no near-miss hint possible.
                assert_eq!(hint, None);
                assert!(
                    alw.is_empty(),
                    "empty allowed-set surfaces verbatim, got {alw:?}"
                );
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_powers_the_derive_emit_end_to_end() {
        // End-to-end path-uniformity: `#[derive(TataraDomain)]` on
        // `MonitorSpec` now emits ONE `parse_kwargs_strict` call in its
        // `compile_from_args` body in place of the prior two-call
        // sequence. The diagnostic identity of an unknown-kwarg
        // rejection from a derived domain MUST equal the diagnostic
        // identity of a direct `parse_kwargs_strict` call on the same
        // args — that's the substrate guarantee the lift establishes:
        // every consumer routes through ONE function. A regression
        // that drifts the derive's emit to re-inline the two-call
        // sequence (or worse, swap them) is structurally observable
        // here as a divergence between the two diagnostic paths.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let derive_err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();

        let args = forms[0].as_list().unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        let strict_err = parse_kwargs_strict(&args[1..], allowed)
            .expect_err("strict call must reject the unknown kwarg");

        match (derive_err, strict_err) {
            (
                LispError::UnknownKwarg {
                    key: dk,
                    hint: dh,
                    allowed: da,
                },
                LispError::UnknownKwarg {
                    key: sk,
                    hint: sh,
                    allowed: sa,
                },
            ) => {
                assert_eq!(dk, sk);
                assert_eq!(dh, sh);
                assert_eq!(da, sa);
            }
            (dother, sother) => panic!(
                "expected matching UnknownKwarg variants, got derive={dother:?} strict={sother:?}",
            ),
        }
    }

    // ── domain-keyed serialize / rewriter-output emission shape ────────
    //
    // The two byte-identical inline `LispError::Compile { form:
    // T::KEYWORD.to_string(), message: format!("serialize…: {e}") }`
    // sites — `register::<T>` (registry-dispatch closure) and
    // `rewrite_typed::<T>` (round-trip prelude) — funnel through
    // `serialize_to_json_err::<T>`. The lone inline non-list-rewriter
    // gate in `rewrite_typed::<T>` funnels through
    // `rewriter_non_list_err::<T>`. These tests pin: (a) the
    // serialize helper produces the structural
    // `LispError::DomainSerialize { keyword: T::KEYWORD, message }`
    // variant — fail-before-pass-after: pre-lift this assertion
    // matched on `LispError::Compile { form, message }` with
    // `form = T::KEYWORD.to_string()`; post-lift the variant identity
    // IS the diagnostic, no substring parse required;
    // (b) the non-list-rewriter helper produces the structural
    // `LispError::RewriterNonList { keyword, got }` variant with
    // `keyword = T::KEYWORD`;
    // (c) Display renders the canonical
    // `"compile error in <keyword>: serialize: …"` / `"compile error
    // in <keyword>: rewriter must return a list; got …"` shape
    // byte-for-byte across the lift so substring-grep consumers see no
    // drift; (d) end-to-end through `rewrite_typed` — a rewriter
    // returning a non-list `Sexp` routes through the helper with the
    // right shape.
    //
    // The redundant-keyword `"serialize {KEYWORD}: …"` shape that
    // `rewrite_typed` used pre-lift is dropped; both sites now render
    // the cleaner `"serialize: …"` shape. The test pins the new
    // canonical form so a regression that re-inlines the old shape
    // fails loudly.

    fn make_serde_err() -> serde_json::Error {
        // Hand-craft a `serde_json::Error` via a known-failing parse so
        // the test exercises the helper's `{e}` Display projection
        // without needing a `Serialize` impl that panics on a real T.
        serde_json::from_str::<i32>("not-a-number").unwrap_err()
    }

    #[test]
    fn serialize_to_json_err_produces_structural_domain_serialize_variant() {
        // Post-lift the helper emits the structural
        // `LispError::DomainSerialize { keyword, message }` variant,
        // not the `Compile`-shaped triple it used to. Fail-before-
        // pass-after: pre-lift this same input emitted
        // `LispError::Compile { form: "defmonitor", message:
        // "serialize: <e>" }` and authoring tools had to substring-
        // grep the rendered diagnostic to recognize this specific
        // gate; post-lift the gate IS its variant identity. `keyword`
        // carries `T::KEYWORD` verbatim (compile-time guarantee load-
        // bearing in the type system); `message` carries the
        // `serde_json::Error::Display` projection unchanged — no
        // `"serialize: "` prefix in the field, the prefix is in
        // `LispError::Display` so consumers binding on the field get
        // the raw underlying message.
        let e = make_serde_err();
        let raw = format!("{e}");
        let err = serialize_to_json_err::<MonitorSpec>(e);
        match err {
            LispError::DomainSerialize { keyword, message } => {
                assert_eq!(keyword, "defmonitor", "keyword must be T::KEYWORD verbatim");
                assert_eq!(
                    message, raw,
                    "message must be the serde_json::Error::Display projection verbatim",
                );
            }
            other => panic!("expected LispError::DomainSerialize, got {other:?}"),
        }
    }

    #[test]
    fn serialize_to_json_err_display_renders_canonical_string() {
        // The Display impl renders `"compile error in <keyword>:
        // serialize: <e>"` — `tatara-check` / REPL / future LSP that
        // substring-grep this shape see no drift across the structural
        // lift, and the redundant keyword repetition (`"serialize
        // defmonitor: …"`) that `rewrite_typed` used pre-canonicalize
        // is gone.
        let e = make_serde_err();
        let raw = format!("{e}");
        let err = serialize_to_json_err::<MonitorSpec>(e);
        let rendered = format!("{err}");
        assert_eq!(
            rendered,
            format!("compile error in defmonitor: serialize: {raw}"),
        );
        // Negative: the pre-canonicalize `"serialize defmonitor: …"`
        // redundant-keyword shape must NOT appear in the new render.
        assert!(
            !rendered.contains("serialize defmonitor:"),
            "redundant-keyword shape must be gone, got: {rendered}"
        );
    }

    #[test]
    fn rewriter_non_list_err_produces_structural_variant() {
        // Post-lift the helper emits the structural
        // `LispError::RewriterNonList { keyword, got: SexpWitness }`
        // variant. `got` is the typed joint identity (`SexpShape` +
        // `Sexp::Display`) — the EIGHTH consumer of the `SexpWitness`
        // primitive, and the FIRST on the typed-EXIT boundary. Tools
        // pattern-match on `got.shape` (structurally) AND read
        // `got.display` (literal) jointly. A regression that re-
        // collapses `got` to a free-form `String` fails-loudly here.
        let got = Sexp::int(42);
        let err = rewriter_non_list_err::<MonitorSpec>(&got);
        match err {
            LispError::RewriterNonList { keyword, got } => {
                assert_eq!(keyword, "defmonitor", "keyword must be T::KEYWORD verbatim");
                assert_eq!(
                    (got.shape, got.display.as_str()),
                    (SexpShape::Int, "42"),
                    "got must carry the typed (SexpShape, Sexp::Display) joint identity",
                );
            }
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
    }

    #[test]
    fn rewriter_non_list_err_display_renders_canonical_string() {
        // The legacy `"rewriter must return a list; got …"` substring
        // shape is preserved byte-for-byte so authoring-tool grep over
        // the rendered diagnostic sees no drift across the lift.
        let got = Sexp::symbol("not-a-list");
        let err = rewriter_non_list_err::<MonitorSpec>(&got);
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: rewriter must return a list; got not-a-list",
        );
    }

    #[test]
    fn rewriter_non_list_err_includes_got_sexp_display() {
        // The `got` payload is projected via the `Sexp` Display impl —
        // pinning a few representative variants keeps the diagnostic's
        // failing-value-naming surface stable across versions. Lists
        // never reach this gate (they short-circuit into the
        // `Sexp::List(items) => items` arm of `rewrite_typed`), but the
        // helper is shape-of-arm — it accepts any non-list `Sexp` the
        // caller hands it. Render strings track the `Sexp::Display`
        // contract verbatim (`Sexp::Nil` → `"()"`, not `"nil"`).
        let cases: &[(Sexp, &str)] = &[
            (Sexp::int(7), "7"),
            (Sexp::string("hi"), "\"hi\""),
            (Sexp::symbol("foo"), "foo"),
            (Sexp::keyword("k"), ":k"),
            (Sexp::Nil, "()"),
        ];
        for (sexp, want_render) in cases {
            let err = rewriter_non_list_err::<MonitorSpec>(sexp);
            let got = match err {
                LispError::RewriterNonList { got, .. } => got,
                other => panic!("expected LispError::RewriterNonList, got {other:?}"),
            };
            assert_eq!(
                got.display, *want_render,
                "Sexp Display projection must thread through unchanged for {sexp:?}"
            );
        }
    }

    #[test]
    fn rewrite_typed_routes_non_list_output_through_helper_e2e() {
        // End-to-end through `rewrite_typed::<MonitorSpec>`: a
        // rewriter returning a non-list `Sexp` (here, an int) MUST
        // route through `rewriter_non_list_err::<MonitorSpec>` and
        // emit a `LispError::RewriterNonList { keyword: "defmonitor",
        // got: "42" }`. Fail-before-pass-after: pre-lift this path
        // emitted `LispError::Compile { ... }` and a regression that
        // re-inlines the shape (or drifts the keyword/got) fails
        // loudly here.
        let input = MonitorSpec {
            name: "x".into(),
            query: "q".into(),
            threshold: 0.5,
            window_seconds: None,
            tags: vec![],
            enabled: None,
        };
        let err = rewrite_typed(input, |_sexp| Ok(Sexp::int(42))).unwrap_err();
        match err {
            LispError::RewriterNonList { keyword, got } => {
                assert_eq!(keyword, "defmonitor");
                assert_eq!((got.shape, got.display.as_str()), (SexpShape::Int, "42"));
            }
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
    }

    #[test]
    fn rewrite_typed_routes_non_list_output_for_every_non_list_variant() {
        // The non-list gate covers EVERY non-list `Sexp` shape — pin
        // a representative sample (atom, quote, unquote-splice)
        // through the gate to confirm the helper is shape-of-arm,
        // not shape-of-some-variants. `Sexp::Nil` renders as `()` per
        // the `Sexp::Display` contract.
        let input = MonitorSpec {
            name: "x".into(),
            query: "q".into(),
            threshold: 0.5,
            window_seconds: None,
            tags: vec![],
            enabled: None,
        };
        let non_lists = [
            Sexp::int(0),
            Sexp::string("bad"),
            Sexp::symbol("not-a-list"),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for bad in non_lists {
            // Each iteration consumes input by cloning the prelude
            // (rewrite_typed takes input by value).
            let clone = MonitorSpec {
                name: input.name.clone(),
                query: input.query.clone(),
                threshold: input.threshold,
                window_seconds: input.window_seconds,
                tags: input.tags.clone(),
                enabled: input.enabled,
            };
            let bad_disp = format!("{bad}");
            let bad_shape = sexp_shape(&bad);
            let err = rewrite_typed(clone, |_sexp| Ok(bad.clone())).unwrap_err();
            match err {
                LispError::RewriterNonList { keyword, got } => {
                    assert_eq!(keyword, "defmonitor");
                    assert_eq!(got.display, bad_disp);
                    assert_eq!(
                        got.shape, bad_shape,
                        "typed SexpShape must thread through for {bad:?}",
                    );
                }
                other => panic!("expected LispError::RewriterNonList, got {other:?}"),
            }
        }
    }

    #[test]
    fn rewrite_typed_well_formed_list_routes_past_non_list_gate() {
        // Positive control — a well-formed list `Sexp` returned by the
        // rewriter routes PAST `rewriter_non_list_err::<T>` cleanly
        // into `T::compile_from_args`. The helper is precisely scoped
        // to non-list `Sexp` outputs; identity-rewriting through the
        // gate preserves the typed value end-to-end. Uses a local
        // single-field domain so the round-trip needs no
        // `#[serde(rename_all)]` plumbing — the production-side
        // round-trip case is covered by
        // `tatara_domains::rewrite_typed_end_to_end`.
        #[derive(DeriveTataraDomain, Serialize, Deserialize, Debug)]
        #[tatara(keyword = "defroundtrip")]
        struct RoundTripSpec {
            name: String,
        }
        let input = RoundTripSpec { name: "x".into() };
        let out = rewrite_typed(input, |sexp| {
            assert!(
                sexp.is_list(),
                "rewriter receives a `Sexp::List` of alternating kwargs"
            );
            Ok(sexp)
        })
        .expect("identity-rewrite of a well-formed typed value must round-trip");
        assert_eq!(out.name, "x");
    }

    #[test]
    fn helpers_are_type_bound_via_t_keyword() {
        // Type-bound symmetry: both helpers project `T::KEYWORD` at the
        // type level — `<T: TataraDomain>` is the boundary, so a typo
        // can never drift the `form` slot across the two call sites in
        // `register::<T>` + `rewrite_typed::<T>`. Pin the projection by
        // exercising the helpers against TWO domains in this module
        // (`MonitorSpec` — defmonitor — and a local domain with a
        // different keyword) and confirm each helper emits the
        // domain's KEYWORD verbatim.
        #[derive(DeriveTataraDomain, Serialize, Debug)]
        #[tatara(keyword = "deflocal")]
        struct LocalSpec {
            name: String,
        }
        // Reference the field so clippy `dead_code` doesn't trip.
        let _local = LocalSpec {
            name: "z".to_string(),
        };
        let e1 = make_serde_err();
        let m_err = serialize_to_json_err::<MonitorSpec>(e1);
        let e2 = make_serde_err();
        let l_err = serialize_to_json_err::<LocalSpec>(e2);
        match m_err {
            LispError::DomainSerialize { keyword, .. } => assert_eq!(keyword, "defmonitor"),
            other => panic!("expected LispError::DomainSerialize, got {other:?}"),
        }
        match l_err {
            LispError::DomainSerialize { keyword, .. } => assert_eq!(keyword, "deflocal"),
            other => panic!("expected LispError::DomainSerialize, got {other:?}"),
        }
        let got = Sexp::int(0);
        match rewriter_non_list_err::<MonitorSpec>(&got) {
            LispError::RewriterNonList { keyword, .. } => assert_eq!(keyword, "defmonitor"),
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
        match rewriter_non_list_err::<LocalSpec>(&got) {
            LispError::RewriterNonList { keyword, .. } => assert_eq!(keyword, "deflocal"),
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
    }

    // ── extract_atom / extract_optional_atom: typed-atom dedup lift ───
    //
    // The eight inline `extract_X` / `extract_optional_X` shapes
    // (`extract_string`, `extract_int`, `extract_float`, `extract_bool`
    // + their optional siblings) all funneled through one of two
    // byte-identical inline `required + project + type_err` triples
    // (required path) or `kw.get + project + type_err` quadruples
    // (optional path). The lift collapses each four-site cluster to
    // ONE named generic primitive (`extract_atom`, `extract_optional_atom`)
    // parameterized by the typed-name label + projection function.
    //
    // The tests below pin: (a) each generic helper's failure-routing
    // surface — missing-required → `MissingKwarg`, present-but-
    // wrong-type → `TypeMismatch` (required path); absent → `Ok(None)`,
    // present-and-correct → `Ok(Some)`, present-but-wrong-type →
    // `TypeMismatch` (optional path); (b) every public delegate
    // (`extract_string`, `extract_int`, `extract_float`, `extract_bool`
    // + optional siblings) routes through the generic helper with the
    // canonical typed-name label intact; (c) Display byte-identity is
    // preserved across the dedup — a regression that drifts the
    // typed-name label (e.g. lowercases `"number"` → `"float"`) fails-
    // loudly at the Display assertion; (d) the borrowed-return path
    // (`extract_string` returns `&'a str` from `&'a Sexp`) round-trips
    // its lifetime through `FnOnce(&'a Sexp) -> Option<&'a str>`
    // cleanly — a regression that breaks the borrow threading fails-
    // to-compile.

    #[test]
    fn extract_atom_propagates_missing_kwarg_via_required() {
        // The required path's first gate — absent kwarg routes through
        // `required` which emits `LispError::MissingKwarg { key }`. Pin
        // the canonical `MissingKwarg` shape and key verbatim; a
        // regression that swallows the gate (e.g. silent `Ok(default)`)
        // or drifts the key slot fails-loudly here. Distinct from
        // `extract_atom_emits_type_mismatch_for_wrong_type` — that
        // pins the second gate.
        let kw: Kwargs<'_> = HashMap::new();
        let err = extract_atom(&kw, "missing", ExpectedKwargShape::Int, Sexp::as_int)
            .expect_err("absent required kwarg must error");
        match err {
            LispError::MissingKwarg { key } => assert_eq!(key, "missing"),
            other => panic!("expected MissingKwarg, got {other:?}"),
        }
    }

    #[test]
    fn extract_atom_emits_type_mismatch_for_wrong_type() {
        // The required path's second gate — present-but-wrong-type
        // kwarg routes through `type_err` which emits
        // `LispError::TypeMismatch { form, expected, got }`. Pin all
        // three slots: `form` is `kwarg_form(key)` (`:wrongkey`),
        // `expected` is the typed-name label fed in verbatim
        // (`"int"`), `got` is `Sexp::Display`'s projection of the
        // offending atom's type (`"string"`). A regression that
        // drifts the typed-name label fails-loudly here.
        let string_sexp = Sexp::string("not-an-int");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("wrongkey".to_string(), &string_sexp);
        let err = extract_atom(&kw, "wrongkey", ExpectedKwargShape::Int, Sexp::as_int)
            .expect_err("present-but-wrong-type kwarg must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("wrongkey".into()));
                assert_eq!(expected, ExpectedKwargShape::Int);
                assert_eq!(got, SexpShape::String);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_atom_returns_value_on_match() {
        // Positive control for `extract_atom` — present and correctly-
        // typed kwarg returns the projected value. Distinct from the
        // two negative paths above; closes the closed set of three
        // outcomes (missing, wrong-type, ok) for the required path.
        let int_sexp = Sexp::int(42);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("count".to_string(), &int_sexp);
        let v = extract_atom(&kw, "count", ExpectedKwargShape::Int, Sexp::as_int)
            .expect("present-and-correct kwarg must succeed");
        assert_eq!(v, 42);
    }

    #[test]
    fn extract_optional_atom_returns_none_for_absent_kwarg() {
        // The optional path's first arm — absent kwarg returns
        // `Ok(None)`, NOT an error. Pin the structural distinction
        // from the required path (which errors on absent) by
        // exercising the same key against both paths; the optional
        // sibling must NEVER call `required` and must NEVER emit
        // `MissingKwarg`. A regression that mistakenly routes the
        // absent arm through `required` would surface here as an
        // `Err(MissingKwarg)` instead of `Ok(None)`.
        let kw: Kwargs<'_> = HashMap::new();
        let v =
            extract_optional_atom::<i64, _>(&kw, "absent", ExpectedKwargShape::Int, Sexp::as_int)
                .expect("absent optional kwarg must succeed with None");
        assert!(v.is_none());
    }

    #[test]
    fn extract_optional_atom_emits_type_mismatch_for_wrong_type() {
        // The optional path's second arm — present-but-wrong-type
        // kwarg errors via `type_err` with the same `TypeMismatch`
        // shape as the required path. Distinct from `extract_atom
        // _emits_type_mismatch_for_wrong_type` only in which kwarg
        // path emitted the error — same variant, same slot
        // semantics. Pins that the optional path does NOT silently
        // swallow type mismatches by returning `Ok(None)` for a
        // present-but-wrong-type kwarg — that would be a typed-entry
        // gate failure.
        let string_sexp = Sexp::string("not-a-bool");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("flag".to_string(), &string_sexp);
        let err =
            extract_optional_atom::<bool, _>(&kw, "flag", ExpectedKwargShape::Bool, Sexp::as_bool)
                .expect_err("present-but-wrong-type optional kwarg must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("flag".into()));
                assert_eq!(expected, ExpectedKwargShape::Bool);
                assert_eq!(got, SexpShape::String);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_optional_atom_returns_some_on_match() {
        // The optional path's third arm — present and correctly-
        // typed kwarg returns `Ok(Some(value))`. Closes the closed
        // set of three outcomes (absent, wrong-type, ok) for the
        // optional path; together with the required-path tests,
        // every distinct extractor outcome is covered.
        let float_sexp = Sexp::float(3.5);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("ratio".to_string(), &float_sexp);
        let v = extract_optional_atom(&kw, "ratio", ExpectedKwargShape::Number, Sexp::as_float)
            .expect("present-and-correct optional kwarg must succeed");
        assert_eq!(v, Some(3.5));
    }

    #[test]
    fn extract_string_borrows_lifetime_through_extract_atom() {
        // The borrowed-return path — `extract_string` returns `&'a str`
        // borrowed from the kwarg `&'a Sexp`. Pins that the lift's
        // `FnOnce(&'a Sexp) -> Option<&'a str>` boundary threads the
        // lifetime correctly: a regression that breaks the
        // higher-ranked lifetime would fail-to-compile (not a runtime
        // assertion). The runtime assertion below pins that the
        // returned `&str` round-trips the kwarg's literal content.
        let s_sexp = Sexp::string("prom-up");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("name".to_string(), &s_sexp);
        let got = extract_string(&kw, "name").expect("present string must succeed");
        assert_eq!(got, "prom-up");
    }

    #[test]
    fn public_extract_delegates_inherit_canonical_type_labels() {
        // Path-uniformity across all four public typed-name labels —
        // `extract_int` (`Int`), `extract_float` (`Number`),
        // `extract_bool` (`Bool`), `extract_string` (`String`). Each
        // delegate must route through `extract_atom` with the
        // canonical typed `ExpectedKwargShape` variant intact; a
        // regression that drifts a label (e.g. `extract_float`'s
        // `Number` → `Int`, or `extract_int`'s `Int` → `Number`)
        // would surface as a `TypeMismatch.expected` variant-identity
        // drift when the extractor is fed a wrong-typed kwarg. After
        // the closed-set lift the typed-enum check is a rustc-enforced
        // contract — a typo in any label literal is unreachable
        // because the variants are the literals.
        let s = Sexp::string("not-typed");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("x".to_string(), &s);
        for (extractor_name, expected_shape, err) in [
            (
                "extract_int",
                ExpectedKwargShape::Int,
                extract_int(&kw, "x").expect_err("must error"),
            ),
            (
                "extract_float",
                ExpectedKwargShape::Number,
                extract_float(&kw, "x").expect_err("must error"),
            ),
            (
                "extract_bool",
                ExpectedKwargShape::Bool,
                extract_bool(&kw, "x").expect_err("must error"),
            ),
        ] {
            match err {
                LispError::TypeMismatch { expected, .. } => assert_eq!(
                    expected, expected_shape,
                    "{extractor_name} must thread the canonical shape {expected_shape:?}",
                ),
                other => panic!("{extractor_name}: expected TypeMismatch, got {other:?}"),
            }
        }
        // `extract_string` against a non-string keyword sexp: same
        // shape, different label. Pinned separately because the
        // string extractor's signature carries a borrow lifetime that
        // doesn't match the tuple shape of the loop above.
        let kw_sexp = Sexp::keyword("not-a-string");
        let mut kw2: Kwargs<'_> = HashMap::new();
        kw2.insert("x".to_string(), &kw_sexp);
        let err = extract_string(&kw2, "x").expect_err("must error");
        match err {
            LispError::TypeMismatch { expected, .. } => {
                assert_eq!(expected, ExpectedKwargShape::String);
            }
            other => panic!("extract_string: expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_atom_renders_legacy_type_mismatch_display() {
        // End-to-end through the `LispError` Display impl — pins that
        // the dedup preserves the legacy `TypeMismatch`-shaped
        // diagnostic byte-for-byte. Authoring tools (`tatara-check`,
        // REPL) that substring-grep on the rendered diagnostic see
        // no drift across the lift. Parallel to how
        // `compile_named_named_form_missing_name_renders_legacy
        // _compile_shape` (compile.rs) pins the lifted helper's
        // Display contract.
        let s = Sexp::string("not-an-int");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("threshold".to_string(), &s);
        let err = extract_int(&kw, "threshold").expect_err("type-mismatch must error");
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected int, got string"
        );
    }

    #[test]
    fn full_monitor_round_trips_through_extract_atom_dedup() {
        // End-to-end positive control: a well-formed defmonitor
        // exercises every typed-atom extractor (`extract_string` on
        // `:name`/`:query`, `extract_float` on `:threshold`,
        // `extract_optional_int` on `:window-seconds`,
        // `extract_optional_bool` on `:enabled`). Pins that the
        // dedup doesn't regress any of the public delegates'
        // semantic — a `MonitorSpec` compiled before and after the
        // lift must produce byte-identical values. Same posture as
        // `derive_compiles_full_form` (the pre-existing positive
        // control); duplicated here to lock the helper-routing
        // invariant.
        let forms = read(
            r#"(defmonitor
                 :name "prom-up"
                 :query "up{job='prometheus'}"
                 :threshold 0.99
                 :window-seconds 300
                 :tags ("prod" "observability")
                 :enabled #t)"#,
        )
        .unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(spec.name, "prom-up");
        assert_eq!(spec.threshold, 0.99);
        assert_eq!(spec.window_seconds, Some(300));
        assert_eq!(spec.enabled, Some(true));
    }

    // ── extract_list: list-typed-kwarg dedup lift ────────────────────
    //
    // `extract_string_list` (each item via `as_string` + `type_err_at`)
    // and `extract_vec_via_serde` (each item via `from_value_with_path`
    // carrying `KwargPath::item`) used to inline the SAME list-extractor
    // skeleton — absent → empty vec, present-but-not-a-list → `type_err`,
    // `iter().enumerate().map(per-item).collect()`. The lift collapses
    // both to ONE generic primitive (`extract_list`) parameterized by the
    // outer-shape label + the per-element projection, the list-family
    // sibling of `extract_atom` / `extract_optional_atom`.
    //
    // The tests below pin the three fixed decisions the skeleton owns:
    // (a) absent kwarg short-circuits to `Ok(Vec::new())` BEFORE any
    // per-item work (the projection is a `panic!` proving it never runs);
    // (b) present-but-not-a-list routes through `type_err` with the
    // CALLER-supplied `list_shape` (tested with `ListOfStrings`, NOT the
    // skeleton-baked `List`, so a regression hardcoding the shape fails
    // loudly) and the per-item projection again never runs; (c) the
    // per-element walk threads the 0-based `enumerate` index into the
    // projection in order; (d) a per-item rejection short-circuits the
    // collect at the FIRST failing element with that element's index in
    // the `KwargPath::Item` slot. The existing `extract_string_list` /
    // `extract_vec_via_serde` suites are the path-uniformity guards
    // proving both public extractors route through it with zero drift.

    #[test]
    fn extract_list_returns_empty_vec_for_absent_kwarg() {
        // Absent list kwarg is the empty list, never an error — same
        // posture `extract_optional_atom` takes for absent atoms. The
        // `panic!` projection proves the absent arm short-circuits
        // BEFORE any per-item work: a regression that walked a (missing)
        // list before the absent check would fire the panic.
        let kw: Kwargs<'_> = HashMap::new();
        let out: Vec<i64> = extract_list(&kw, "absent", ExpectedKwargShape::List, |_, _| {
            panic!("per-item projection must not run for an absent list kwarg")
        })
        .expect("absent list kwarg must succeed with an empty vec");
        assert!(out.is_empty());
    }

    #[test]
    fn extract_list_emits_type_err_with_caller_supplied_list_shape() {
        // Present-but-not-a-list routes through the outer-shape gate with
        // the CALLER's `list_shape` (`ListOfStrings`), not a skeleton-baked
        // `List` — a regression hardcoding the shape fails here. The
        // `panic!` projection also proves the per-item walk never starts
        // when the outer gate rejects.
        let scalar = Sexp::int(5);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("tags".to_string(), &scalar);
        let err =
            extract_list::<String, _>(&kw, "tags", ExpectedKwargShape::ListOfStrings, |_, _| {
                panic!("per-item projection must not run when the outer shape gate fails")
            })
            .expect_err("present-but-not-a-list kwarg must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("tags".into()));
                assert_eq!(expected, ExpectedKwargShape::ListOfStrings);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_list_threads_enumerate_index_into_projection_in_order() {
        // The per-element walk threads the 0-based `enumerate` index into
        // the projection and collects results in order. Pin both the index
        // sequence (0, 1, 2) and the element order so a regression that
        // dropped `.enumerate()` or reordered the walk fails loudly.
        let items = Sexp::List(vec![
            Sexp::string("a"),
            Sexp::string("b"),
            Sexp::string("c"),
        ]);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("xs".to_string(), &items);
        let out: Vec<(usize, String)> =
            extract_list(&kw, "xs", ExpectedKwargShape::List, |idx, e| {
                Ok((
                    idx,
                    e.as_string().expect("test items are strings").to_string(),
                ))
            })
            .expect("well-formed list must collect");
        assert_eq!(
            out,
            vec![
                (0, "a".to_string()),
                (1, "b".to_string()),
                (2, "c".to_string()),
            ]
        );
    }

    #[test]
    fn extract_list_short_circuits_at_first_failing_item_with_its_index() {
        // A per-item rejection short-circuits the collect at the FIRST
        // failing element, carrying that element's index in the
        // `KwargPath::Item` slot. The third element (`"never"`) is a valid
        // string but must never be reached — index 1 (the int) fails first.
        let items = Sexp::List(vec![
            Sexp::string("ok"),
            Sexp::int(9),
            Sexp::string("never"),
        ]);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("xs".to_string(), &items);
        let err =
            extract_list::<String, _>(&kw, "xs", ExpectedKwargShape::ListOfStrings, |idx, e| {
                e.as_string()
                    .map(String::from)
                    .ok_or_else(|| type_err_at("xs", idx, ExpectedKwargShape::String, e))
            })
            .expect_err("a non-string item must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(
                    form,
                    crate::error::KwargPath::Item {
                        key: "xs".into(),
                        idx: 1,
                    }
                );
                assert_eq!(expected, ExpectedKwargShape::String);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }
}
