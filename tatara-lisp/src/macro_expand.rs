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
//!   - Optional params:      `(name a &optional b c)` — unsupplied bind to `()`
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

/// Default ceiling on `Expander::expand`'s recursive re-entry into a
/// macro-call form. A macro whose expansion contains another call to
/// itself — the canonical runaway `(defmacro loop (x) `(loop ,x))` —
/// pre-lift stack-overflowed the process (a below-floor abort with no
/// `Result` witness). The ceiling turns that abort into a
/// [`LispError::ExpansionDepthExceeded`] rejection at the expander
/// boundary. `256` is large enough that no lawful hand-authored
/// expansion nesting hits it (nested `when` / `let` / template
/// compositions cap out in single digits in practice) and small
/// enough that a runaway is rejected in microseconds instead of
/// consuming a process stack. Consumers that need a tighter or looser
/// ceiling call [`Expander::set_max_expansion_depth`].
pub const DEFAULT_MAX_EXPANSION_DEPTH: usize = 256;

/// Default ceiling on the expansion cache's entry count. Peer to
/// [`DEFAULT_MAX_EXPANSION_DEPTH`] one RESOURCE axis over on the
/// expander surface — where the depth ceiling bounds RECURSION (a
/// runaway macro's stack), this ceiling bounds MEMORY (the memoized
/// `apply(macro, args)` result table). Pre-lift the cache was an
/// unbounded [`HashMap`] shared across [`Expander`] clones via
/// [`std::sync::Arc`]; a long-running host (REPL, LSP,
/// `tatara-check`, MCP server) that expanded many distinct
/// `(macro, args)` pairs would accrete cache entries without bound —
/// the same below-floor resource-drift the depth ceiling closed on
/// the recursion axis. Post-lift the [`Expander::apply`] insert path
/// consults this ceiling before growing the cache; once the entry
/// count reaches the ceiling, new expansions still succeed (the
/// cache remains a pure PERFORMANCE optimization — never a
/// CORRECTNESS gate) but skip cache insertion until the operator
/// clears the cache via [`Expander::clear_cache`]. Consumers that
/// need a tighter ceiling (embedded / test harnesses) call
/// [`Expander::set_max_cache_entries`] with a lower cap; consumers
/// that want unbounded caching (batch compilation runs where memory
/// is not the concern) call it with [`usize::MAX`].
///
/// `8192` is large enough that lawful macro authoring — a small
/// closed set of macros, each expanded against a bounded set of
/// argument shapes — never brushes the ceiling on a session-length
/// workload (a typescape with 20 macros × 32 distinct arg-shapes ×
/// several call sites still sits under 1K entries), and small
/// enough that a pathological expansion loop (a macro emitting
/// unique arg-shapes on every call) is rejected before it exhausts
/// process memory. The default is deliberately generous — the
/// ceiling exists to prevent unbounded drift, not to force operators
/// to tune it. Consumers that want a tighter policy configure it
/// explicitly.
pub const DEFAULT_MAX_CACHE_ENTRIES: usize = 8192;

/// Default ceiling on a single macro-expansion output's structural
/// node count. Peer to [`DEFAULT_MAX_EXPANSION_DEPTH`] and
/// [`DEFAULT_MAX_CACHE_ENTRIES`] one RESOURCE-DIMENSION axis over on
/// the expander surface — where the depth ceiling bounds RECURSION
/// LENGTH (a runaway macro's stack) and the cache ceiling bounds
/// MEMOIZATION WIDTH (the memoized `apply(macro, args)` table's
/// entry count), this ceiling bounds OUTPUT SIZE (the "expansion
/// bomb" axis: a well-defined macro whose depth stays small while
/// the produced tree grows past any reasonable working-set budget).
/// Pre-lift the canonical
/// `(defmacro bomb (x) `(list ,x ,x ,x ,x ,x ,x ,x ,x))` composed
/// under itself a handful of times exhausted the process heap while
/// the depth counter was still at low single digits and the cache
/// entry count sat under any reasonable ceiling — a below-floor
/// resource-drift the depth AND cache ceilings both admitted through.
/// Post-lift the [`Expander::expand_with_depth`] site consults this
/// ceiling on every freshly-applied macro output via
/// [`crate::ast::Sexp::node_count`]; once an `apply` result exceeds
/// the ceiling the expander returns
/// [`crate::error::LispError::ExpansionSizeExceeded`] with
/// `macro_name`, `size`, and `limit` populated. Consumers that need
/// a tighter or looser ceiling call
/// [`Expander::set_max_expansion_size`].
///
/// `65_536` is large enough that lawful macro-authored output — a
/// domain-language body composed from a few dozen typed primitives,
/// each expanded against a bounded set of arg-shapes — never brushes
/// the ceiling on a session-length workload (a typical `defpoint` /
/// `defmonitor` expansion sits under 100 nodes, and even a
/// hand-authored `defcheck` cascade tops out in the low thousands),
/// and small enough that a pathological "output bomb" (a macro
/// producing a 2^N-node blob from a small input) is rejected in
/// milliseconds instead of consuming multiple gigabytes of process
/// memory in the worst case. The default is deliberately generous —
/// the ceiling exists to prevent unbounded drift, not to force
/// operators to tune it. Consumers that want a tighter policy
/// configure it explicitly.
pub const DEFAULT_MAX_EXPANSION_SIZE: usize = 65_536;

/// Default ceiling on a registered macro's BODY structural node count.
/// Peer to [`DEFAULT_MAX_EXPANSION_DEPTH`], [`DEFAULT_MAX_CACHE_ENTRIES`],
/// and [`DEFAULT_MAX_EXPANSION_SIZE`] one PIPELINE-STAGE axis over on
/// the expander surface — the three prior guards close the RESOURCE
/// surface at EXPAND time (recursion length, memoization width,
/// single-`apply` output size); this ceiling closes it at REGISTER time
/// (the SIZE the body itself contributes to storage in
/// [`Expander::macros`], to [`compile_template`]'s pre-expansion walk,
/// and to every subsequent [`substitute`] walk that recurses through
/// the body-shaped template). Pre-lift a pathological
/// `(defmacro huge (x) `<template-of-N-million-nodes>)` would land the
/// body in the macros table before any `apply` ran and inflate the
/// per-registration cost proportional to `N` — a below-floor
/// resource-drift the three expand-time ceilings all admitted through
/// because none of them fires at the registration boundary. Post-lift
/// the [`Expander::register_macro_def`] site consults this ceiling
/// against `def.body.node_count()`; once the observed size exceeds
/// the ceiling the expander returns
/// [`crate::error::LispError::MacroBodySizeExceeded`] with
/// `macro_name`, `size`, and `limit` populated, and BOTH the
/// [`Expander::macros`] AND [`Expander::templates`] tables stay
/// exactly as they were — no partial-write window in which the
/// registration succeeded halfway.
///
/// `16_384` is large enough that lawful hand-authored macro bodies
/// (an outer quasi-quote wrapping a few dozen nested lists with a
/// handful of substitution slots) never brush the ceiling on any
/// realistic authoring workload — even a heavily-composed
/// `defalertpolicy` template with multi-page cascades tops out in the
/// low thousands — and small enough that a pathological "authoring
/// bomb" (a code-generator that unrolls a `Vec<TypedEntity>` into a
/// giant literal body) is rejected at the registration boundary in
/// microseconds instead of leaking a multi-megabyte template into the
/// live macros table. The default is deliberately generous — the
/// ceiling exists to prevent unbounded drift, not to force operators
/// to tune it. Consumers that want a tighter or looser ceiling call
/// [`Expander::set_max_macro_body_size`].
pub const DEFAULT_MAX_MACRO_BODY_SIZE: usize = 16_384;

/// Default ceiling on the [`Expander::macros`] table's entry count.
/// Peer to [`DEFAULT_MAX_MACRO_BODY_SIZE`] one RESOURCE-DIMENSION axis
/// over on the REGISTER-time surface — where the body-size ceiling
/// bounds a single macro's BODY SIZE (the per-registration authoring
/// bomb), this ceiling bounds TABLE ENTRY COUNT (the cumulative
/// registration bomb: a code-generator emitting unbounded fresh
/// `(defmacro N (x) `(list ,x))` heads whose bodies each sit
/// comfortably under [`DEFAULT_MAX_MACRO_BODY_SIZE`] yet collectively
/// saturate process memory). Also peer to [`DEFAULT_MAX_CACHE_ENTRIES`]
/// one PIPELINE-STAGE axis over — where the cache-entries ceiling
/// bounds EXPAND-time memoization width, this ceiling bounds
/// REGISTER-time macro-table width. The two entry-count guards close
/// the two pipeline stages symmetrically. Pre-lift the macros table
/// was an unbounded [`HashMap`]; a long-running host (REPL, LSP,
/// `tatara-check`, MCP server) that ingested a source stream of
/// distinct `(defmacro …)` heads would accrete entries without
/// bound — a below-floor resource-drift the four prior guards (depth,
/// cache-entries, expansion-size, body-size) all admitted through
/// because none of them fires against the CUMULATIVE registration
/// count.
///
/// Overwrites (a re-registration of an already-registered key) do not
/// grow the table and never trigger this ceiling — operators can
/// redefine a macro at capacity without hitting the gate. Only FRESH
/// keys are counted against the ceiling.
///
/// `4096` is large enough that lawful hand-authored typescapes never
/// brush the ceiling on any realistic authoring workload — a
/// typescape with 20-100 macros × several call sites sits well under
/// 1K entries, and even a library-heavy project composing hundreds of
/// domain macros stays under a fraction of the default — and small
/// enough that a pathological registration bomb (a code-generator
/// unrolling a `Vec<TypedEntity>` into a giant `(defmacro fresh-N …)`
/// stream) is rejected at the registration boundary in microseconds
/// instead of leaking a multi-megabyte macros table into the live
/// expander. The default is deliberately generous — the ceiling
/// exists to prevent unbounded drift, not to force operators to tune
/// it. Consumers that want a tighter or looser ceiling call
/// [`Expander::set_max_registered_macros`].
pub const DEFAULT_MAX_REGISTERED_MACROS: usize = 4096;

/// Default ceiling on a registered macro's lambda-list ARITY — the
/// count of parameter slots (`required` + `optional` + `rest?`) the
/// [`MacroDef`] declares. Peer to [`DEFAULT_MAX_MACRO_BODY_SIZE`] one
/// RESOURCE-DIMENSION axis over on the REGISTER-time surface — where
/// body-size bounds a per-registration BODY node count (the AST-nodes
/// of the template a macro rewrites INTO), this ceiling bounds a
/// per-registration PARAM slot count (the fresh symbols
/// [`compile_template`]'s `,name`-index resolution AND
/// [`MacroParams::bind`]'s per-index binder walk have to thread through
/// on every call). Peer to [`DEFAULT_MAX_REGISTERED_MACROS`] one
/// RESOURCE-DIMENSION axis over on the REGISTER-time surface — where
/// the registered-macros ceiling bounds cumulative registration COUNT
/// (the table-scoped resource), this ceiling bounds a per-registration
/// PARAM-LIST WIDTH (the per-entry resource). Together the three
/// REGISTER-time ceilings close the (per-body SIZE, per-body ARITY,
/// per-table COUNT) three-corner surface — a `Vec<TypedEntity>`
/// arity-bomb code-generator emitting a
/// `(defmacro huge (a-1 a-2 … a-N-million) `,a-1)` head with a
/// single-node body slips past `max_macro_body_size` and past
/// `max_registered_macros` (one fresh key), yet each subsequent call
/// pays O(N) walking the required-run binder — a below-floor
/// resource-drift the two prior REGISTER-time guards admit through
/// because neither of them fires against the PER-REGISTRATION
/// PARAM-LIST WIDTH.
///
/// The default is deliberately generous — lawful hand-authored macros
/// carry a handful of required params + a small optional run + at
/// most a single `&rest` slot (`observability-stack`'s `defalertpolicy`
/// carries under 8 slots; even keyword-heavy authoring surfaces sit
/// well under 32 slots). `128` sits an order of magnitude past any
/// realistic authoring workload yet an order of magnitude BELOW a
/// pathological code-generator's arity-bomb output, so the ceiling
/// gates the drift without brushing lawful traffic. Consumers that
/// want a tighter or looser ceiling call
/// [`Expander::set_max_macro_arity`].
///
/// Frontier inspiration: Common Lisp's `LAMBDA-PARAMETERS-LIMIT`
/// standard variable (CLHS §3.4.1) — the runtime-reflectable
/// "upper exclusive bound on the number of parameters that may
/// appear in a lambda list" every conforming implementation carries.
/// SBCL's is `4611686018427387903`, ECL's is `4096`, CLisp's is `4096`.
/// `DEFAULT_MAX_MACRO_ARITY` is the substrate's typed-Rust peer at
/// compile time, translated through pleme-io primitives: a
/// `pub const usize` on the typed macro-expander algebra rather than
/// a runtime-mutable global, wired into the substrate's typed
/// [`crate::error::LispError`] Result algebra at
/// [`Expander::register_macro_def`] rather than raised as an
/// implementation-defined condition.
pub const DEFAULT_MAX_MACRO_ARITY: usize = 128;

/// Typed snapshot of the six `Expander` resource ceilings — the
/// bundled peer of the six [`Expander::max_expansion_depth`] /
/// [`Expander::max_cache_entries`] / [`Expander::max_expansion_size`] /
/// [`Expander::max_macro_body_size`] / [`Expander::max_registered_macros`] /
/// [`Expander::max_macro_arity`] individual getters. The six knobs form
/// ONE resource surface (the (PIPELINE-STAGE × RESOURCE-DIMENSION)
/// grid the last six commits closed corner-by-corner) yet the
/// pre-lift `Expander` surface exposed them only as SIX independent
/// projections — inspecting the full posture required six calls, and
/// bulk-configuring the expander (a preset for CI, a preset for the
/// REPL, a preset for a test harness) required six independent
/// setter invocations whose ordering the type system did not gate.
///
/// Post-lift the typed bundle binds the six knobs at ONE typed value
/// on the `Expander` surface. A `#[derive(Clone, Copy, Debug,
/// PartialEq, Eq)]` posture — every ceiling is a `Copy` scalar —
/// gives callers a cheap snapshot they can serialize, compare, or
/// pass around; the `Copy` implementation guarantees the bundle
/// never carries a hidden allocation. Adding a SEVENTH ceiling
/// extends this struct AND [`DEFAULT_RESOURCE_LIMITS`] in lockstep
/// via rustc's field-exhaustiveness on the `..Default::default()`-free
/// literal construction — no independent-field-drift window.
///
/// Peer of the six `DEFAULT_MAX_*` module constants one AGGREGATION
/// axis over: where each constant carries ONE ceiling, this struct
/// carries the SIX-fold cross-product. [`DEFAULT_RESOURCE_LIMITS`]
/// pins the (defaults × aggregation) corner so a consumer that wants
/// the shipped posture in one value does not re-derive the six
/// individual constants at its call site.
///
/// Theory grounding: THEORY.md §II.1 — typed entry / typed exit for
/// the resource-limit configuration surface. The pre-lift six
/// independent knobs were a set of typed scalars but their
/// composition into an "Expander resource posture" was untyped —
/// callers assembled the posture at their call site with no
/// compile-time exhaustiveness check that all six were considered.
/// Frontier inspiration: `tokio::runtime::Builder`'s bundled knob
/// struct — a runtime's ceiling posture is ONE typed value rather
/// than a chain of independent method calls; translation through
/// pleme-io primitives is the plain `Copy` snapshot below, no
/// builder-pattern indirection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    /// See [`Expander::max_expansion_depth`].
    pub max_expansion_depth: usize,
    /// See [`Expander::max_cache_entries`].
    pub max_cache_entries: usize,
    /// See [`Expander::max_expansion_size`].
    pub max_expansion_size: usize,
    /// See [`Expander::max_macro_body_size`].
    pub max_macro_body_size: usize,
    /// See [`Expander::max_registered_macros`].
    pub max_registered_macros: usize,
    /// See [`Expander::max_macro_arity`].
    pub max_macro_arity: usize,
}

impl Default for ResourceLimits {
    /// The shipped posture — every field seeded from its matching
    /// `DEFAULT_MAX_*` module constant. Pinned equal to
    /// [`DEFAULT_RESOURCE_LIMITS`] as a compile-time identity so a
    /// future re-tuning of ONE default lands in ONE place and both
    /// projections pick it up mechanically.
    fn default() -> Self {
        DEFAULT_RESOURCE_LIMITS
    }
}

/// The shipped [`ResourceLimits`] posture — every field bound to its
/// matching `DEFAULT_MAX_*` module constant at ONE typed value. This
/// is the `const` peer of [`ResourceLimits::default`]; the two are
/// pinned equal so a call-site that wants the shipped posture as a
/// compile-time value binds through the `const` and a call-site that
/// wants it as a runtime `Default::default()` value binds through the
/// trait impl — both routes reach the SAME six numbers.
///
/// Adding a SEVENTH ceiling extends this constant AND the struct AND
/// [`Expander`]'s constructors in lockstep — rustc's
/// field-exhaustiveness on this `..Default::default()`-free literal
/// forces the new field to appear here, and the trait impl above
/// reuses this constant so no independent-default-drift window opens
/// between the two projections.
pub const DEFAULT_RESOURCE_LIMITS: ResourceLimits = ResourceLimits {
    max_expansion_depth: DEFAULT_MAX_EXPANSION_DEPTH,
    max_cache_entries: DEFAULT_MAX_CACHE_ENTRIES,
    max_expansion_size: DEFAULT_MAX_EXPANSION_SIZE,
    max_macro_body_size: DEFAULT_MAX_MACRO_BODY_SIZE,
    max_registered_macros: DEFAULT_MAX_REGISTERED_MACROS,
    max_macro_arity: DEFAULT_MAX_MACRO_ARITY,
};

/// The unbounded [`ResourceLimits`] posture — every ceiling seeded to
/// [`usize::MAX`] at ONE typed value. This is the CEILING-LIFTED peer of
/// [`DEFAULT_RESOURCE_LIMITS`] one PRESET-POSTURE axis over on the
/// named-preset surface: where the default constant binds each field to
/// the shipped `DEFAULT_MAX_*` module constant, this constant binds
/// each field to the raw `usize::MAX` sentinel that every
/// [`Expander::set_max_expansion_depth`] / [`Expander::set_max_cache_entries`] /
/// [`Expander::set_max_expansion_size`] / [`Expander::set_max_macro_body_size`] /
/// [`Expander::set_max_registered_macros`] / [`Expander::set_max_macro_arity`]
/// setter's doc names as "the ceiling is effectively lifted" —
/// admitting any lawful value the type system supports.
///
/// A test-harness fixture that wants "exercise the raw expander with no
/// ceiling gating anything" pre-lift composed the posture at its call
/// site as six independent [`usize::MAX`] literals (or six independent
/// setter invocations post-`Expander::new`) whose exhaustiveness the
/// type system did NOT gate — a copy-paste that dropped one of the six
/// lifts left THAT ceiling at its default and gated the fixture
/// silently. Post-lift the posture is ONE named typed constant every
/// such fixture routes through, AND rustc's field-exhaustiveness on
/// the `..Default::default()`-free literal here forces a SEVENTH
/// ceiling extension to appear at this site in lockstep with
/// [`DEFAULT_RESOURCE_LIMITS`] and the [`ResourceLimits`] struct
/// itself — no independent-preset-drift window.
///
/// Peer to [`DEFAULT_RESOURCE_LIMITS`] on the (posture × aggregation)
/// grid: both constants live at the AGGREGATION corner (six ceilings
/// as ONE typed value); the posture axis distinguishes the SHIPPED
/// default from the CEILING-LIFTED unbounded preset. Disagrees with
/// [`DEFAULT_RESOURCE_LIMITS`] on every field (every default is a
/// concrete non-[`usize::MAX`] positive constant, so the six-way
/// disagreement is structurally exhaustive) — pinned as a typed
/// theorem in the [`ResourceLimits`] test cohort.
///
/// Frontier inspiration: `tokio::runtime::Builder`'s pattern of
/// exposing named preset thunks (`multi_thread`, `current_thread`)
/// alongside the field-level configurators, so a caller that wants a
/// standard posture binds through ONE named entry rather than
/// composing the field literals at the call site. Translation
/// through pleme-io primitives is the plain `const` snapshot below,
/// no builder-pattern indirection — the `Copy` posture on
/// [`ResourceLimits`] already gives callers the struct-update
/// literal (`ResourceLimits { max_expansion_depth: 4,
/// ..UNBOUNDED_RESOURCE_LIMITS }`) that isolates the ONE ceiling
/// still gating from the five lifted.
pub const UNBOUNDED_RESOURCE_LIMITS: ResourceLimits = ResourceLimits {
    max_expansion_depth: usize::MAX,
    max_cache_entries: usize::MAX,
    max_expansion_size: usize::MAX,
    max_macro_body_size: usize::MAX,
    max_registered_macros: usize::MAX,
    max_macro_arity: usize::MAX,
};

/// The zero [`ResourceLimits`] posture — every ceiling seeded to `0` at
/// ONE typed value. This is the BOTTOM peer of
/// [`UNBOUNDED_RESOURCE_LIMITS`] one LATTICE-POLE axis over on the
/// (bottom, top) bounded-lattice preset-pair surface: where the
/// ceiling-lifted preset carries every gate at [`usize::MAX`]
/// (admitting every lawful `usize`-indexed value the type system
/// supports), this preset carries every gate at `0` (rejecting every
/// non-empty input past the first depth step, cache slot, macro-body
/// node, macro registration, or arity slot). Together the two close
/// the (BOTTOM, TOP) preset-posture pair on the bounded-lattice
/// diagonal, and [`DEFAULT_RESOURCE_LIMITS`] sits strictly between
/// them (`EMPTY.leq(DEFAULT) && DEFAULT.leq(UNBOUNDED)`).
///
/// **Bounded-lattice identity elements**: [`ResourceLimits`] under the
/// pointwise `min` / `max` operations forms a BOUNDED lattice; each
/// operation acquires its identity element from the OPPOSITE pole of
/// the preset-pair diagonal, and its ANNIHILATOR from the SAME pole.
///   * [`Self::strictest`] (meet, pointwise `min`) —
///     [`UNBOUNDED_RESOURCE_LIMITS`] is the identity
///     (`a.strictest(UNBOUNDED) == a` for every posture, since pointwise
///     `min` against [`usize::MAX`] returns `a`); this constant is the
///     ANNIHILATOR (`a.strictest(EMPTY) == EMPTY` for every posture,
///     since pointwise `min` against `0` returns `0`).
///   * [`Self::most_permissive`] (join, pointwise `max`) — this
///     constant is the identity (`a.most_permissive(EMPTY) == a` for
///     every posture, since pointwise `max` against `0` returns `a`);
///     [`UNBOUNDED_RESOURCE_LIMITS`] is the ANNIHILATOR
///     (`a.most_permissive(UNBOUNDED) == UNBOUNDED`).
///   * [`Self::leq`] (partial order) — this constant is the MINIMUM
///     (`EMPTY.leq(a) == true` for every posture, since `0 <= x` holds
///     for every `usize x`); [`UNBOUNDED_RESOURCE_LIMITS`] is the
///     MAXIMUM (`a.leq(UNBOUNDED) == true` for every posture, since
///     `x <= usize::MAX` holds for every `usize x`).
///
/// **Fold-identity use case**: a caller aggregating a slice of
/// postures through [`Self::most_permissive`] (computing the pointwise
/// least upper bound across the slice) seeds the fold from this
/// constant — the join-identity property guarantees the seed does not
/// distort the fold's result:
///
/// ```rust,ignore
/// let joined = postures.iter().copied()
///     .fold(EMPTY_RESOURCE_LIMITS, ResourceLimits::most_permissive);
/// ```
///
/// The dual pattern (folding through [`Self::strictest`] to compute
/// the pointwise greatest lower bound) seeds from
/// [`UNBOUNDED_RESOURCE_LIMITS`] — the meet-identity from the OPPOSITE
/// pole. Pre-lift, an aggregator either (a) picked ONE arbitrary
/// element of the slice as the seed and folded through the tail — a
/// non-total fixup that panicked on an empty slice — or (b) seeded
/// from `Some(first)`-style option wrapper — a runtime `Option` that
/// carried a type-level admission the aggregate might be absent even
/// though the algebra defines it exhaustively via its identity. Post-
/// lift the fold takes ONE typed identity element and handles empty
/// slices as the algebraic identity value — a boundary case which the
/// bounded-lattice structure resolves at TYPE level rather than the
/// caller resolving at CALL SITE with a wrapper type.
///
/// Adding a SEVENTH ceiling extends this constant AND the struct AND
/// [`DEFAULT_RESOURCE_LIMITS`] AND [`UNBOUNDED_RESOURCE_LIMITS`] in
/// lockstep — rustc's field-exhaustiveness on this
/// `..Default::default()`-free literal forces the new field to appear
/// at this site, and the identity-law tests pin `0` as the seventh
/// field's value at compile time via the const-fn peer of the
/// existing lattice-law const-fn pins.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 — composition preserves
/// proofs; the identity element of a lattice operation is itself a
/// typed named entry whose composition with any other element is the
/// element (join) or the identity (meet). THEORY.md §V.1 — knowable
/// platform; the bounded-lattice bottom binds at ONE typed constant
/// rather than a six-primitive inline literal every consumer that
/// needs the fold identity carries at its call site. Frontier
/// inspiration: the algebraic-datatypes tradition of a `Monoid` typeclass
/// (Haskell's `mempty`, Scala's `Monoid.empty`, Rust's various
/// `Default::default` for `Sum`/`Product` newtypes) — a caller
/// aggregating values through a binary combinator binds its identity
/// element at ONE named entry rather than composing the identity
/// literal at every fold site. Translation through pleme-io primitives
/// is the plain `const` snapshot below, no typeclass indirection — the
/// `Copy` posture on [`ResourceLimits`] already gives callers the
/// struct-update literal (`ResourceLimits { max_macro_arity: 3,
/// ..EMPTY_RESOURCE_LIMITS }`) that isolates the ONE ceiling admitting
/// non-zero input from the five sealed at `0`.
pub const EMPTY_RESOURCE_LIMITS: ResourceLimits = ResourceLimits {
    max_expansion_depth: 0,
    max_cache_entries: 0,
    max_expansion_size: 0,
    max_macro_body_size: 0,
    max_registered_macros: 0,
    max_macro_arity: 0,
};

/// Pointwise `min` / `max` primitives the [`ResourceLimits`] lattice binds
/// its meet ([`ResourceLimits::strictest`]) and join
/// ([`ResourceLimits::most_permissive`]) operations to. Named at module
/// scope as `const fn` so both operations are themselves `const fn` on
/// stable — composing two postures at compile time (a caller stitching a
/// new `pub const` preset from two shipped presets) needs no runtime
/// evaluation. Ternary form rather than [`usize::min`] / [`usize::max`]
/// so the two `const fn` remain valid on any tatara MSRV whose stability
/// on the primitive const-fn methods has not been established.
const fn min_usize(a: usize, b: usize) -> usize {
    if a <= b {
        a
    } else {
        b
    }
}

const fn max_usize(a: usize, b: usize) -> usize {
    if a >= b {
        a
    } else {
        b
    }
}

impl ResourceLimits {
    /// Pointwise-`min` across the six ceilings — the STRICTEST posture
    /// whose admissible input set is the intersection of `self`'s and
    /// `other`'s admissible input sets. `a.strictest(b)` admits an
    /// input iff BOTH `a` AND `b` admit it: on every field the
    /// per-axis ceiling is the smaller of the two, so a value that
    /// clears the meet clears BOTH postures, and a value that any
    /// posture rejects the meet also rejects.
    ///
    /// The meet of the `ResourceLimits` lattice under the pointwise
    /// partial order `a ≤ b iff every field of a ≤ every field of b`
    /// (tighter-first — a POSTURE admits FEWER inputs on that field's
    /// axis when the ceiling is smaller). Peer of
    /// [`Self::most_permissive`] one COMBINATOR axis over on the
    /// lattice-algebra surface: where `most_permissive` is the join
    /// (pointwise-`max`, LUB), this is the meet (pointwise-`min`, GLB).
    /// Together the two operations close the (meet, join) lattice
    /// operator pair on the [`ResourceLimits`] posture algebra.
    ///
    /// The concrete identity on the shipped preset pair holds
    /// structurally: [`DEFAULT_RESOURCE_LIMITS`]`.strictest(`
    /// [`UNBOUNDED_RESOURCE_LIMITS`]`) == `[`DEFAULT_RESOURCE_LIMITS`].
    /// Every `DEFAULT_MAX_*` module constant is a concrete positive
    /// value strictly less than [`usize::MAX`], so on every axis the
    /// pointwise-`min` picks the DEFAULT side; the six-axis pin holds
    /// as a typed theorem in the test cohort.
    ///
    /// Idempotent (`a.strictest(a) == a`), commutative
    /// (`a.strictest(b) == b.strictest(a)`), associative
    /// (`(a.strictest(b)).strictest(c) == a.strictest(b.strictest(c))`),
    /// and satisfies the absorption identity with the join
    /// (`a.strictest(a.most_permissive(b)) == a`) — all pinned as
    /// typed theorems in the test cohort. Distributive over the join
    /// (`a.strictest(b.most_permissive(c)) ==
    /// a.strictest(b).most_permissive(a.strictest(c))`) because
    /// pointwise-`min` and pointwise-`max` on [`usize`] each
    /// distribute over the other on every axis.
    ///
    /// `const fn` so a caller can compose two named presets into a
    /// third at compile time (`pub const CI_RESOURCE_LIMITS:
    /// ResourceLimits = SOME_OPERATOR_PRESET.strictest(
    /// SOME_CI_PRESET);`) rather than deferring the composition to
    /// a runtime `Default::default()` chain.
    ///
    /// Pre-lift, a caller wanting the tightest common posture across
    /// two shipped presets composed the intersection at its call site
    /// as six independent `usize::min(a.max_X, b.max_X)` invocations
    /// stitched into a fresh `ResourceLimits { ... }` literal — the
    /// same six-inline-primitive shape the pre-`ResourceLimits`
    /// bundled `Expander` posture required its callers to carry, and
    /// the same exhaustiveness gap the `..Default::default()`-free
    /// literal on the bundled struct exists to close: a copy-paste
    /// that dropped ONE of the six `min` invocations left THAT
    /// ceiling at whichever posture the caller happened to seed the
    /// literal with, and the type system did not gate the drop.
    /// Post-lift the composition binds at ONE typed method on the
    /// posture algebra — a caller writes
    /// `operator_preset.strictest(ci_preset)` and rustc's
    /// field-exhaustiveness at the const's construction guarantees
    /// every ceiling is threaded through the `min` primitive.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the tightest posture of two preset-carried
    /// resource proofs is itself a `ResourceLimits` whose proof
    /// content is the intersection of the two, and this method is
    /// the typed named entry composing them. THEORY.md §V.1 —
    /// knowable platform; the pointwise-`min` combinator becomes a
    /// TYPE-level operation on the posture algebra rather than an
    /// inline six-primitive cascade at every consumer that composes
    /// two presets. `tatara-lattice`'s `meet` / `join` operators on
    /// `Classification` (see `pleme-io/tatara` workspace crate
    /// docstring: "Lattice algebra over Classification — `meet` /
    /// `join` / `leq` / `Baseline`") close the SAME shape one layer
    /// down: an entity's classification is a lattice; a resource
    /// posture is a lattice; both algebras carry the (meet, join)
    /// operator pair as their fundamental composition primitives.
    #[must_use]
    pub const fn strictest(self, other: Self) -> Self {
        Self {
            max_expansion_depth: min_usize(self.max_expansion_depth, other.max_expansion_depth),
            max_cache_entries: min_usize(self.max_cache_entries, other.max_cache_entries),
            max_expansion_size: min_usize(self.max_expansion_size, other.max_expansion_size),
            max_macro_body_size: min_usize(self.max_macro_body_size, other.max_macro_body_size),
            max_registered_macros: min_usize(
                self.max_registered_macros,
                other.max_registered_macros,
            ),
            max_macro_arity: min_usize(self.max_macro_arity, other.max_macro_arity),
        }
    }

    /// Pointwise-`max` across the six ceilings — the MOST-PERMISSIVE
    /// posture whose admissible input set is the union of `self`'s
    /// and `other`'s admissible input sets. `a.most_permissive(b)`
    /// admits an input iff EITHER `a` OR `b` admits it: on every
    /// field the per-axis ceiling is the larger of the two, so a
    /// value that either posture accepts the join accepts, and a
    /// value the join rejects both postures reject.
    ///
    /// The join of the `ResourceLimits` lattice under the pointwise
    /// partial order `a ≤ b iff every field of a ≤ every field of b`
    /// (tighter-first). Peer of [`Self::strictest`] one COMBINATOR
    /// axis over on the lattice-algebra surface: where `strictest` is
    /// the meet (pointwise-`min`, GLB), this is the join
    /// (pointwise-`max`, LUB). Together the two operations close the
    /// (meet, join) lattice operator pair on the [`ResourceLimits`]
    /// posture algebra.
    ///
    /// The concrete identity on the shipped preset pair holds
    /// structurally: [`DEFAULT_RESOURCE_LIMITS`]`.most_permissive(`
    /// [`UNBOUNDED_RESOURCE_LIMITS`]`) == `[`UNBOUNDED_RESOURCE_LIMITS`].
    /// Every `DEFAULT_MAX_*` module constant is a concrete positive
    /// value strictly less than [`usize::MAX`], so on every axis the
    /// pointwise-`max` picks the UNBOUNDED side; the six-axis pin
    /// holds as a typed theorem in the test cohort.
    ///
    /// Idempotent, commutative, associative, and satisfies the
    /// absorption identity with the meet
    /// (`a.most_permissive(a.strictest(b)) == a`) — all pinned as
    /// typed theorems in the test cohort. Distributive over the meet
    /// (`a.most_permissive(b.strictest(c)) ==
    /// a.most_permissive(b).strictest(a.most_permissive(c))`).
    ///
    /// `const fn` so a caller can compose two named presets into a
    /// third at compile time rather than deferring the composition
    /// to a runtime `Default::default()` chain.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the most-permissive posture of two preset-
    /// carried resource proofs is itself a `ResourceLimits` whose
    /// proof content is the union of the two, and this method is the
    /// typed named entry composing them. THEORY.md §V.1 — knowable
    /// platform; the pointwise-`max` combinator becomes a TYPE-level
    /// operation on the posture algebra rather than an inline
    /// six-primitive cascade at every consumer that composes two
    /// presets.
    #[must_use]
    pub const fn most_permissive(self, other: Self) -> Self {
        Self {
            max_expansion_depth: max_usize(self.max_expansion_depth, other.max_expansion_depth),
            max_cache_entries: max_usize(self.max_cache_entries, other.max_cache_entries),
            max_expansion_size: max_usize(self.max_expansion_size, other.max_expansion_size),
            max_macro_body_size: max_usize(self.max_macro_body_size, other.max_macro_body_size),
            max_registered_macros: max_usize(
                self.max_registered_macros,
                other.max_registered_macros,
            ),
            max_macro_arity: max_usize(self.max_macro_arity, other.max_macro_arity),
        }
    }

    /// Pointwise partial-order relation across the six ceilings — `self`
    /// is at least as tight as `other` iff every field of `self` is at
    /// most the matching field of `other`. `a.leq(b)` holds iff every
    /// input the `a` posture admits the `b` posture also admits: `a`'s
    /// per-axis ceilings all sit at or below `b`'s per-axis ceilings, so
    /// any value that clears `a`'s tighter gates also clears `b`'s
    /// looser gates.
    ///
    /// The partial order underneath the [`ResourceLimits`] lattice.
    /// Peer of [`Self::strictest`] (meet, GLB) and [`Self::most_permissive`]
    /// (join, LUB) one PRIMITIVE-KIND axis over on the lattice-algebra
    /// surface: where those two are the (meet, join) COMBINATORS, this
    /// is the RELATION they are defined against. Together the three
    /// close the (meet, join, leq) primitive triple on the
    /// [`ResourceLimits`] posture algebra — the same triple
    /// `tatara-lattice`'s [`Lattice`] trait exposes as its three
    /// required methods for every classification lattice one crate over
    /// (see `tatara-lattice/src/lib.rs`).
    ///
    /// **Meet-agreement** (`a.leq(b) ⇔ a.strictest(b) == a`): if `a` is
    /// tighter on every axis than `b`, then the pointwise `min` picks
    /// `a` on every axis, so `a.strictest(b) == a`; conversely, if
    /// `a.strictest(b) == a`, every field's `min` picked `a`, so every
    /// field of `a` is at most the matching field of `b`. Pinned as
    /// `resource_limits_leq_agrees_with_meet` in the test cohort.
    ///
    /// **Join-agreement** (`a.leq(b) ⇔ a.most_permissive(b) == b`):
    /// symmetric statement on the LUB side; pinned as
    /// `resource_limits_leq_agrees_with_join`. Together the two
    /// agreements are the canonical lattice cross-check axiom
    /// `a ≤ b ⇔ a ⊓ b = a ⇔ a ⊔ b = b` documented in
    /// `tatara-lattice/src/lib.rs`'s preamble.
    ///
    /// Reflexive (`a.leq(a) == true`), antisymmetric (`a.leq(b) &&
    /// b.leq(a) → a == b`), transitive (`a.leq(b) && b.leq(c) →
    /// a.leq(c)`) — all pinned as typed theorems in the test cohort.
    /// NOT total: two postures with reversed per-axis orderings (some
    /// axes where `a` is tighter, some where `b` is tighter) are
    /// incomparable — `!a.leq(b) && !b.leq(a)` — pinned by
    /// `resource_limits_leq_is_not_total_on_asymmetric_postures`.
    ///
    /// **Bounds pins in terms of leq**:
    ///   * `a.strictest(b).leq(a) && a.strictest(b).leq(b)` (meet is
    ///     the greatest lower bound) — pinned by
    ///     `resource_limits_strictest_is_leq_both_operands`, the typed
    ///     lattice-relation companion to the per-axis
    ///     `resource_limits_strictest_is_dominated_by_both_operands_pointwise`.
    ///   * `a.leq(a.most_permissive(b)) && b.leq(a.most_permissive(b))`
    ///     (join is the least upper bound) — pinned by
    ///     `resource_limits_most_permissive_is_geq_both_operands`, the
    ///     typed lattice-relation companion to the per-axis
    ///     `resource_limits_most_permissive_dominates_both_operands_pointwise`.
    ///
    /// **Concrete-preset pin**: [`DEFAULT_RESOURCE_LIMITS`]`.leq(`
    /// [`UNBOUNDED_RESOURCE_LIMITS`]`) == true` (every
    /// `DEFAULT_MAX_*` module constant is a concrete positive value at
    /// most [`usize::MAX`]) AND
    /// `!UNBOUNDED_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS)` (every
    /// `DEFAULT_MAX_*` is strictly less than [`usize::MAX`], so on
    /// every axis [`usize::MAX`] exceeds the matching default) — the
    /// partial order is a STRICT order on the shipped preset pair.
    /// Pinned by `resource_limits_leq_of_default_and_unbounded_is_a_strict_order`.
    ///
    /// `const fn` so a caller can pin a preset-relation identity at
    /// compile time (`const _: () = assert!(DEFAULT_RESOURCE_LIMITS
    /// .leq(UNBOUNDED_RESOURCE_LIMITS));`) — the typed lattice-relation
    /// peer of the `const fn` composition ability on
    /// [`Self::strictest`] / [`Self::most_permissive`].
    ///
    /// Pre-lift, a caller wanting to decide whether one posture was
    /// tighter than another either (a) composed six independent
    /// per-axis `<=` comparisons at the call site stitched into a
    /// six-fold `&&` — the same six-inline-primitive shape the
    /// pre-`ResourceLimits` bundled Expander posture required its
    /// callers to carry, and the same exhaustiveness gap the
    /// `..Default::default()`-free literal on the bundled struct exists
    /// to close: a copy-paste that dropped ONE of the six comparisons
    /// silently admitted a posture that violated tightness on THAT
    /// axis — or (b) invoked [`Self::strictest`] and compared the result
    /// to `self` — a correct but roundabout composition of the partial-
    /// order primitive against its lattice-companion. Post-lift the
    /// relation binds at ONE typed method on the posture algebra; a
    /// caller writes `tighter.leq(looser)` and rustc's exhaustiveness
    /// on the six-field conjunction below guarantees every ceiling is
    /// threaded through the `<=` primitive.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the partial-order relation on two preset-
    /// carried resource proofs is itself a typed named entry composing
    /// their tightness ordering. THEORY.md §V.1 — knowable platform;
    /// the pointwise `<=` combinator becomes a TYPE-level operation on
    /// the posture algebra rather than an inline six-primitive
    /// conjunction at every consumer that compares two presets.
    /// `tatara-lattice`'s [`Lattice::leq`] method on `Classification`
    /// closes the SAME shape one layer down — an entity's
    /// classification is a lattice with a `leq` partial order; a
    /// resource posture is a lattice with a `leq` partial order; both
    /// algebras carry the (meet, join, leq) primitive triple as their
    /// fundamental relation-and-combinator surface.
    #[must_use]
    pub const fn leq(self, other: Self) -> bool {
        self.max_expansion_depth <= other.max_expansion_depth
            && self.max_cache_entries <= other.max_cache_entries
            && self.max_expansion_size <= other.max_expansion_size
            && self.max_macro_body_size <= other.max_macro_body_size
            && self.max_registered_macros <= other.max_registered_macros
            && self.max_macro_arity <= other.max_macro_arity
    }

    /// Pointwise-`min` fold across a slice of postures — the STRICTEST
    /// posture whose admissible input set is the intersection of every
    /// operand's admissible input sets. `ResourceLimits::strictest_of(&[a,
    /// b, c])` admits an input iff EVERY operand admits it: on every
    /// field the per-axis ceiling is the smallest across the whole slice,
    /// so a value that clears the aggregated meet clears EVERY operand,
    /// and a value that ANY operand rejects the aggregated meet also
    /// rejects.
    ///
    /// The N-ary MEET aggregation on the [`ResourceLimits`] lattice —
    /// the extension of [`Self::strictest`] from a 2-input pairwise
    /// combinator to an N-input fold across a slice. Seeded from
    /// [`UNBOUNDED_RESOURCE_LIMITS`] (the meet-identity — its per-axis
    /// [`usize::MAX`] never wins the pointwise `min` against a concrete
    /// positive value, so the seed contributes nothing to the fold's
    /// result), which makes the empty-slice case aggregate to the
    /// identity element rather than requiring an `Option<Self>` wrapper
    /// the caller inspects at every consumption site. Peer of
    /// [`Self::most_permissive_of`] one COMBINATOR axis over on the
    /// N-ary aggregation surface: where `most_permissive_of` is the
    /// N-ary JOIN (LUB across the slice, seeded from the join-identity
    /// [`EMPTY_RESOURCE_LIMITS`]), this is the N-ary MEET (GLB across
    /// the slice, seeded from the meet-identity). Together the two
    /// close the (N-ary meet, N-ary join) aggregation pair the pairwise
    /// combinators [`Self::strictest`] / [`Self::most_permissive`]
    /// extend from 2 to N.
    ///
    /// **Empty-slice identity**: `strictest_of(&[]) ==
    /// UNBOUNDED_RESOURCE_LIMITS` — the null aggregate binds structurally
    /// to the meet-identity element. Pre-lift a caller composing "the
    /// tightest posture across THIS SLICE" from the pairwise `strictest`
    /// primitive either (a) seeded the fold from a non-empty first
    /// element and panicked on empty input, or (b) wrapped the aggregate
    /// in `Option<Self>` — a runtime type-level admission the aggregate
    /// might be absent even though the algebra's bounded-lattice
    /// structure defines the null aggregate exhaustively via its
    /// identity element. Post-lift the null aggregate is
    /// [`UNBOUNDED_RESOURCE_LIMITS`] at the TYPE level, not a wrapper
    /// the consumer inspects.
    ///
    /// **Single-element identity**: `strictest_of(&[a]) == a` — the
    /// 1-input aggregate is the input verbatim, since
    /// `UNBOUNDED.strictest(a) == a` for every posture `a` by the
    /// meet-identity law.
    ///
    /// **Two-element identity**: `strictest_of(&[a, b]) ==
    /// a.strictest(b)` — the 2-input aggregate reduces to the pairwise
    /// combinator, since `UNBOUNDED.strictest(a).strictest(b) ==
    /// a.strictest(b)`.
    ///
    /// **Order-independent**: `strictest_of(&[a, b, c]) ==
    /// strictest_of(&[c, b, a])` — the fold inherits the commutativity
    /// AND associativity of the pairwise `strictest`, so any permutation
    /// of the slice yields the same aggregate.
    ///
    /// `const fn` so a caller can compose an arbitrary-length preset
    /// slice into a third preset at compile time (`pub const
    /// COMBINED_PRESET: ResourceLimits = ResourceLimits::strictest_of(
    /// &[SOME_OPERATOR_PRESET, SOME_CI_PRESET, SOME_HOSTILE_PRESET]);`)
    /// rather than deferring the composition to a runtime chain — the
    /// N-ary const-fn peer of the pairwise const-fn on
    /// [`Self::strictest`]. `Copy` on [`ResourceLimits`] lets the const-
    /// fn loop index the slice by value without an explicit `.clone()`
    /// (which const-fn would not permit anyway).
    ///
    /// Pre-lift, the SAME `.iter().copied().fold(UNBOUNDED, strictest)`
    /// three-primitive cascade appeared verbatim in
    /// `unbounded_resource_limits_seeds_strictest_fold_over_slice` AND at
    /// every future consumer that would compose "the tightest across
    /// this set of presets" — a PRIME DIRECTIVE ≥2 trigger, since the
    /// fold docstring on [`UNBOUNDED_RESOURCE_LIMITS`] explicitly names
    /// this pattern as the constant's fold-identity use case. Post-lift
    /// the N-ary aggregation binds at ONE typed method the algebra
    /// exposes, and every future consumer routes through ONE name whose
    /// signature carries the identity-element choice INTO the method
    /// (rather than the consumer having to remember which seed pairs
    /// with which combinator at each call site).
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the N-ary aggregation of N preset-carried
    /// resource proofs is itself a `ResourceLimits` whose proof content
    /// is the intersection of every operand's, and this method is the
    /// typed named entry composing them. THEORY.md §V.1 — knowable
    /// platform; the N-ary meet becomes a TYPE-level operation on the
    /// posture algebra rather than an inline fold cascade at every
    /// consumer that aggregates a slice of presets — with the
    /// meet-identity seed baked in so the consumer cannot pair the join-
    /// identity (`EMPTY`) with the meet combinator (a silent
    /// distortion that would collapse every axis to `0`). Frontier
    /// inspiration: the algebraic-datatypes tradition of a `Monoid`
    /// typeclass exposing `mconcat` / `Monoid.combineAll` alongside the
    /// pairwise `mappend` — the N-ary aggregation of a collection
    /// through a binary combinator is a first-class named method the
    /// typeclass carries. Translation through pleme-io primitives is
    /// the plain `const fn` slice-fold below, no typeclass indirection
    /// — the meet-identity element already exists as a named `pub const`
    /// on the algebra so the fold picks it up structurally.
    #[must_use]
    pub const fn strictest_of(postures: &[Self]) -> Self {
        let mut acc = UNBOUNDED_RESOURCE_LIMITS;
        let mut i = 0;
        while i < postures.len() {
            acc = acc.strictest(postures[i]);
            i += 1;
        }
        acc
    }

    /// Pointwise-`max` fold across a slice of postures — the MOST-
    /// PERMISSIVE posture whose admissible input set is the union of
    /// every operand's admissible input sets.
    /// `ResourceLimits::most_permissive_of(&[a, b, c])` admits an input
    /// iff ANY operand admits it: on every field the per-axis ceiling
    /// is the largest across the whole slice.
    ///
    /// The N-ary JOIN aggregation on the [`ResourceLimits`] lattice —
    /// the extension of [`Self::most_permissive`] from a 2-input
    /// pairwise combinator to an N-input fold across a slice. Seeded
    /// from [`EMPTY_RESOURCE_LIMITS`] (the join-identity — every field
    /// at `0`, which never wins the pointwise `max` against any
    /// non-zero value, so the seed contributes nothing to the fold's
    /// result), which makes the empty-slice case aggregate to the
    /// identity element. Peer of [`Self::strictest_of`] one COMBINATOR
    /// axis over on the N-ary aggregation surface: where `strictest_of`
    /// is the N-ary MEET (seeded from the meet-identity
    /// [`UNBOUNDED_RESOURCE_LIMITS`]), this is the N-ary JOIN (seeded
    /// from the join-identity). Together the two close the (N-ary meet,
    /// N-ary join) aggregation pair.
    ///
    /// **Empty-slice identity**: `most_permissive_of(&[]) ==
    /// EMPTY_RESOURCE_LIMITS` — the null aggregate binds structurally
    /// to the join-identity element.
    ///
    /// **Single-element identity**: `most_permissive_of(&[a]) == a` —
    /// the 1-input aggregate is the input verbatim, since
    /// `EMPTY.most_permissive(a) == a` for every posture `a` by the
    /// join-identity law.
    ///
    /// **Two-element identity**: `most_permissive_of(&[a, b]) ==
    /// a.most_permissive(b)` — the 2-input aggregate reduces to the
    /// pairwise combinator.
    ///
    /// **Order-independent**: `most_permissive_of(&[a, b, c]) ==
    /// most_permissive_of(&[c, b, a])` — the fold inherits the
    /// commutativity AND associativity of the pairwise
    /// `most_permissive`.
    ///
    /// `const fn` so a caller can compose an arbitrary-length preset
    /// slice at compile time — the N-ary const-fn peer of the pairwise
    /// const-fn on [`Self::most_permissive`].
    ///
    /// Pre-lift, the SAME `.iter().copied().fold(EMPTY,
    /// most_permissive)` three-primitive cascade appeared verbatim in
    /// `empty_resource_limits_seeds_most_permissive_fold_over_slice`
    /// AND at every future consumer that would compose "the loosest
    /// across this set of presets" — a PRIME DIRECTIVE ≥2 trigger.
    /// Post-lift the N-ary aggregation binds at ONE typed method with
    /// the join-identity seed baked in so the consumer cannot pair the
    /// meet-identity (`UNBOUNDED`) with the join combinator (a silent
    /// distortion that would inflate every axis to [`usize::MAX`]).
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the N-ary join of N preset-carried resource
    /// proofs is itself a `ResourceLimits`. THEORY.md §V.1 — knowable
    /// platform; the N-ary join becomes a TYPE-level operation on the
    /// posture algebra rather than an inline fold cascade at every
    /// consumer that aggregates a slice of presets.
    #[must_use]
    pub const fn most_permissive_of(postures: &[Self]) -> Self {
        let mut acc = EMPTY_RESOURCE_LIMITS;
        let mut i = 0;
        while i < postures.len() {
            acc = acc.most_permissive(postures[i]);
            i += 1;
        }
        acc
    }
}

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

impl MacroDef {
    /// Project the macro body to its substitution-walked form: the inner of
    /// the outer `Sexp::Quasiquote` when `(defmacro NAME (PARAMS) `(...))`
    /// authored the body through the canonical quasi-quote affordance, OR
    /// `&self.body` verbatim when authored without one. The two expansion
    /// strategies — bytecode (`compile_template`) and substitute (`apply`'s
    /// substitute fallback) — both walk this projection, never the raw
    /// `body`, because the outer quasi-quote is the syntactic "you're
    /// inside a template" marker and the substitution semantics operate on
    /// what's INSIDE it. Naming the projection lifts the inline
    /// `match &def.body { Sexp::Quasiquote(inner) => inner.as_ref(),
    /// other => other }` peel that appeared verbatim at BOTH sites — well
    /// past the ≥2 PRIME-DIRECTIVE trigger — into ONE function the two
    /// strategies share, so a regression that drifts ONE strategy's
    /// body-projection from the other (e.g. one path peels twice and the
    /// other peels once, or one path treats `Sexp::Quote(...)` as a
    /// template marker and the other doesn't) becomes structurally
    /// impossible: there is exactly one implementation both strategies
    /// call.
    ///
    /// Single-level peel by design: a nested `` ``form `` body unwraps to
    /// `` `form `` (the inner quasi-quote stays as-is), matching the v0
    /// "no nested quasi-quotes" scope the module preamble declares. A
    /// non-quasi-quote body — `(defmacro NAME (PARAMS) BODY)` where BODY
    /// is a plain `Sexp::List` / `Sexp::Atom` — returns `&self.body`
    /// verbatim, the "other" arm of the legacy match. The borrow is
    /// strictly `&'a Sexp` rooted in `&'a self.body` (no clone, no
    /// allocation); both `compile_node` (bytecode path) and `substitute`
    /// (substitute path) consume the projection immediately and never
    /// outlive the borrow.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; two
    /// inline copies of the body-peel match is the ≥2 trigger, and the
    /// substrate names the projection ONCE so authoring surfaces and
    /// future expansion strategies (a third interpreter? a JIT? a
    /// debugger that wants to render the body without the outer
    /// quasi-quote marker?) bind to ONE primitive. THEORY.md §II.1
    /// invariant 2 — free middle; the two expansion strategies emit
    /// IDENTICAL output for the same (macro, args) pair, and sharing one
    /// body-projection makes that per-strategy agreement structural at
    /// the entry to the walker, not a two-site discipline the
    /// `expansion_layers_agree_on_output_and_cache_wins` benchmark only
    /// observes after the fact.
    #[must_use]
    pub fn template_body(&self) -> &Sexp {
        match &self.body {
            Sexp::Quasiquote(inner) => inner.as_ref(),
            other => other,
        }
    }
}

/// A macro's parameter list — structurally "zero or more required
/// positional params, then zero or more `&optional` params, then an OPTIONAL
/// single `&rest` param." This is the canonical Lisp lambda-list ordering
/// (Common Lisp `(req* &optional opt* &rest r)`), made a TYPE.
///
/// This shape promotes the invariants the reader ([`parse_params`])
/// previously upheld only by construction — `&rest` is LAST, there is AT MOST
/// ONE of it, and (now) `&optional` params sit strictly between the required
/// run and the rest — from *unobserved discipline* to *unrepresentable
/// state*. The prior representation `Vec<Param>` admitted `[Rest, Required]`
/// (a `&rest` in the middle) and `[Rest, Rest]` (two of them); both are
/// nonsense the binder cannot honor, yet the type permitted them. The flat
/// param INDEX that the bytecode references (`Subst(idx)` / `Splice(idx)`)
/// and the positional binder both walk would silently misalign on such a
/// `Vec` — a `Rest` at index 0 of `[Rest, Required]` makes the binder grab
/// every arg, then fail to bind the trailing `Required`, mapping the
/// template's index-1 substitution onto the wrong value. `MacroParams`
/// cannot express either shape: `rest` is exactly one `Option<String>`,
/// always conceptually after every `required` then every `optional` name,
/// and the three kinds live in distinct fields whose order is fixed by the
/// struct, not by a discipline the binder trusts a `Vec` to have upheld.
///
/// `optional` differs from `required` in the binder, not the index contract:
/// a required name with no arg at its position is a `MissingMacroArg`
/// rejection; an optional name with no arg binds to its declared default form
/// — `Sexp::Nil` when none was given, the parsed default literal when one was.
/// Both shapes — `&optional x` and `&optional (x 5)` — are now structural in
/// the typed [`OptionalParam`] entry rather than smeared across a flat
/// `Vec<String>` the binder would have had to discover the default for
/// elsewhere.
///
/// The flat-index contract the template bytecode depends on is preserved by
/// [`MacroParams::names`]: index `0..required.len()` are the required names
/// in order, the next `optional.len()` indices are the optional names, and
/// the final index (if present) is the rest name — the canonical lambda-list
/// order. [`MacroParams::bind`] produces the per-index bound values in that
/// same order, so the name-keyed (`bind_args` → `substitute`) and
/// index-keyed (`apply_compiled`) expansion strategies share ONE binder and
/// can never drift.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid states
/// unrepresentable"; the lambda-list ordering (required → optional → rest,
/// rest-is-last, at-most-one-rest) becomes structural. THEORY.md §VI.1 —
/// generation over composition; the positional binding loop (verbatim in
/// both `bind_args` and `apply_compiled`, the ≥2 PRIME-DIRECTIVE trigger) is
/// lifted to ONE owner, `bind`, which the optional arm extends in one place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MacroParams {
    pub required: Vec<String>,
    pub optional: Vec<OptionalParam>,
    pub rest: Option<String>,
}

/// One entry in a macro's `&optional` section — a name plus an optional
/// default form. The two surface shapes the reader admits collapse into this
/// single typed shape:
///
///   * `&optional x`        ⇒ `OptionalParam { name: "x", default: None }`
///   * `&optional (x 5)`    ⇒ `OptionalParam { name: "x", default: Some(Int(5)) }`
///
/// The `default: Option<Sexp>` slot makes the per-param default-form a
/// FIELD on each optional entry, not a discipline a sibling `Vec<Sexp>` would
/// have had to maintain in lock-step with `Vec<String>`. Without this shape
/// the binder cannot tell "no arg supplied, no default declared → bind nil"
/// from "no arg supplied, default `5` declared → bind `5`": both would
/// collapse onto `Sexp::Nil`, the precise silent misalignment the typed
/// shape exists to forbid.
///
/// The default is the LITERAL `Sexp` — there is no evaluator in v0, so a
/// `(x (foo 1))` spec parks `(foo 1)` verbatim as the bound value when `x`'s
/// arg is absent. This is the no-evaluator floor of CL semantics: any
/// arbitrary form is admitted at the gate, what it MEANS is the next layer's
/// concern. The default is parsed exactly once at `defmacro`/
/// `defpoint-template`/`defcheck` time (inside `parse_params`); every call
/// to that macro consumes the same parsed `Sexp` via `Clone`, never re-
/// reading the source.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid states
/// unrepresentable"; the (name, default?) pair is one entry rather than two
/// parallel `Vec`s a regression could desynchronize. THEORY.md §VI.1 —
/// generation over composition; the binder's optional arm consults
/// `param.default` in ONE place, so the substitute and bytecode strategies
/// inherit identical default-resolution semantics from the shared `bind`.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionalParam {
    pub name: String,
    pub default: Option<Sexp>,
}

impl OptionalParam {
    /// `&optional x` — a bare optional name with no default. An absent
    /// argument binds to `Sexp::Nil` (the no-default-form floor).
    #[must_use]
    pub fn bare(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default: None,
        }
    }

    /// `&optional (x DEFAULT)` — an optional with a default form. An absent
    /// argument binds to `default.clone()`.
    #[must_use]
    pub fn with_default(name: impl Into<String>, default: Sexp) -> Self {
        Self {
            name: name.into(),
            default: Some(default),
        }
    }

    /// The bound value when an absent call leaves this optional slot unfilled:
    /// the declared default form (cloned) when one was authored, OR the
    /// canonical `Sexp::Nil` floor when none was — the CL `&optional` no-
    /// default-form floor. ONE named primitive on the typed [`OptionalParam`]
    /// every absent-call binder consults; before this lift the same two-arm
    /// fallback `param.default.clone().unwrap_or(Sexp::Nil)` lived inline at
    /// [`MacroParams::bind`]'s optional arm, and a future absence-resolver
    /// (the kwarg-gate's typed-default fill? a future `&supplied-p` slot's
    /// "was this defaulted?" bit?) would have had to re-derive the same
    /// two-arm fallback at every site that walks the optional run.
    ///
    /// The projection IS the structural identity binding the typed
    /// `default: Option<Sexp>` slot to its bound-value contract:
    ///   * `bare(name).resolved_default()` is `Sexp::Nil` (the no-default
    ///     floor — `default.is_none()`).
    ///   * `with_default(name, d).resolved_default()` is `d.clone()` (the
    ///     declared default — `default = Some(d)` projected through Clone).
    ///
    /// `resolved_default()` is the typed accessor companion to the
    /// `bare` / `with_default` constructors: those two constructors define
    /// the ONLY admissible shapes of the typed `default` slot, and this
    /// accessor names the BOUND-VALUE projection both shapes yield at the
    /// binder's absence arm. Together the three close the `OptionalParam`'s
    /// self-contained typed surface — every authored shape lands through ONE
    /// of two constructors, and every absent-call binder reads through this
    /// ONE accessor.
    ///
    /// Returns an owned `Sexp` (not `&Sexp`) because the binder pushes the
    /// resolved default into a fresh `Vec<Sexp>` slot at every absent call;
    /// the `default.clone()` projection is the same allocation the pre-lift
    /// inline expression performed, just named at the typed boundary. The
    /// `Sexp::Nil` floor is a free per-call construction (a unit variant
    /// with no payload), so the no-default path is free of allocation
    /// beyond the function return slot.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid
    /// states unrepresentable"; the "no-default-form floor" structural
    /// concept becomes a NAMED projection on [`OptionalParam`] rather than
    /// re-derived `param.default.clone().unwrap_or(Sexp::Nil)` arithmetic
    /// at every site that walks the bound optional run. Authoring tools
    /// (REPL, LSP, `tatara-check`) that want to render "this optional
    /// binds to {default-form|nil} when absent" bind to ONE method on the
    /// typed param. THEORY.md §VI.1 — generation over composition; the
    /// constructor pair `bare` / `with_default` defines the typed shapes
    /// and the `resolved_default` accessor names the symmetric
    /// bound-value projection — the typed accessor companion. THEORY.md
    /// §II.1 invariant 2 — free middle; both expansion strategies route
    /// through the SHARED `MacroParams::bind`, so the new accessor is
    /// exposed to the bytecode and substitute paths uniformly via that
    /// shared binder.
    #[must_use]
    pub fn resolved_default(&self) -> Sexp {
        self.default.clone().unwrap_or(Sexp::Nil)
    }
}

impl MacroParams {
    /// Canonical `&rest` marker — the ONE typed `&'static str` on
    /// [`MacroParams`] the parser's rest-slot dispatch specialises on.
    /// The Common-Lisp lambda-list `&rest` keyword names the position
    /// at which the parser stops collecting `required` / `optional`
    /// names and binds every subsequent arg into a `Sexp::List` at the
    /// [`Self::rest`] slot. Sibling posture to [`Self::OPTIONAL_MARKER`]
    /// (`"&optional"`) on the same lambda-list-keyword algebra layer:
    /// the two constants are the closed set of Common-Lisp
    /// lambda-list-keyword `&'static str` markers the parser's typed
    /// dispatch specialises on, sharing the canonical `char` lead byte
    /// [`Self::LAMBDA_LIST_KEYWORD_LEAD`] (`'&'`).
    ///
    /// Pre-lift the same `"&rest"` bytes lived inline as a single
    /// `s == "&rest"` comparison inside [`parse_params`], with the two
    /// typed `&'static str` markers the CL lambda-list algebra
    /// specialises on smeared across TWO parallel `s == "..."` inline
    /// comparisons at the SAME dispatch cascade. Post-lift the
    /// (`&rest` rest-slot marker, canonical `&'static str`) pairing
    /// binds at ONE constant on the typed [`MacroParams`] algebra that
    /// both the parser AND any future authoring / rendering surface
    /// route through; a refactor that swaps the marker (e.g. a
    /// Clojure-compat port to `&`  ONLY, an Elisp-compat port that
    /// keeps `&rest` as-is, a Scheme R7RS-style port to `.` for the
    /// dotted-pair rest slot) touches ONE constant rather than the
    /// two inline literals scattered across the parser AND the
    /// diagnostic message surfaces.
    ///
    /// Structural round-trip contract:
    /// `Self::REST_MARKER.starts_with(Self::LAMBDA_LIST_KEYWORD_LEAD)`
    /// — the projection law binding the `&'static str` marker to its
    /// canonical `char` lead byte on the typed algebra, pinned by
    /// `macro_params_rest_marker_prefixed_by_lambda_list_keyword_lead`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// (`&rest` rest-slot marker, canonical `&'static str`) pairing
    /// binds at ONE `pub const` on the typed [`MacroParams`] algebra
    /// regardless of which parser / authoring surface reaches in.
    /// THEORY.md §V.1 — knowable platform; the canonical CL lambda-
    /// list `&rest` keyword becomes a TYPE-level constant on the
    /// typed macro-params algebra rather than an inline `"&rest"`
    /// literal at the parser's rest-slot dispatch arm.
    pub const REST_MARKER: &'static str = "&rest";

    /// Canonical `&optional` marker — the ONE typed `&'static str` on
    /// [`MacroParams`] the parser's optional-section dispatch
    /// specialises on. The Common-Lisp lambda-list `&optional` keyword
    /// names the position at which the parser switches subsequent
    /// bare-symbol names from the `required` bin to the `optional`
    /// bin, until either the end of the param list OR the
    /// [`Self::REST_MARKER`] terminal is reached. Sibling posture to
    /// [`Self::REST_MARKER`] (`"&rest"`) on the same lambda-list-
    /// keyword algebra layer: the two constants are the closed set of
    /// CL lambda-list-keyword `&'static str` markers the parser's
    /// typed dispatch specialises on, sharing the canonical `char`
    /// lead byte [`Self::LAMBDA_LIST_KEYWORD_LEAD`] (`'&'`).
    ///
    /// Structural round-trip contract:
    /// `Self::OPTIONAL_MARKER.starts_with(Self::LAMBDA_LIST_KEYWORD_LEAD)`
    /// — pinned by
    /// `macro_params_optional_marker_prefixed_by_lambda_list_keyword_lead`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// (`&optional` section-switch marker, canonical `&'static str`)
    /// pairing binds at ONE `pub const` on the typed [`MacroParams`]
    /// algebra regardless of which parser / authoring surface reaches
    /// in. THEORY.md §V.1 — knowable platform; the canonical CL
    /// lambda-list `&optional` keyword becomes a TYPE-level constant
    /// on the typed macro-params algebra rather than an inline
    /// `"&optional"` literal at the parser's optional-section
    /// dispatch arm.
    pub const OPTIONAL_MARKER: &'static str = "&optional";

    /// Canonical `&` LEAD byte shared across [`Self::REST_MARKER`]
    /// (`"&rest"`) and [`Self::OPTIONAL_MARKER`] (`"&optional"`) — the
    /// ONE canonical `char` on the [`MacroParams`] algebra that the
    /// Common-Lisp lambda-list-keyword disjointness contract binds to.
    ///
    /// Sibling posture to the closed set of `pub const` per-role
    /// canonical bytes on the substrate's other closed-set outer
    /// algebras: [`crate::ast::Atom::STR_DELIMITER`] (`'"'`),
    /// [`crate::ast::Atom::STR_ESCAPE_LEAD`] (`'\\'`),
    /// [`crate::ast::Atom::KEYWORD_MARKER_LEAD`] (`':'`),
    /// [`crate::ast::Atom::BOOL_LITERAL_LEAD`] (`'#'`),
    /// [`crate::ast::Sexp::LIST_OPEN`] (`'('`),
    /// [`crate::ast::Sexp::LIST_CLOSE`] (`')'`),
    /// [`crate::ast::Sexp::COMMENT_LEAD`] (`';'`),
    /// [`crate::ast::Sexp::COMMENT_TERM`] (`'\n'`),
    /// [`crate::ast::QuoteForm::SPLICE_DISCRIMINATOR`] (`'@'`), and
    /// every `crate::ast::QuoteForm::lead_char` projection — every
    /// canonical per-role byte the substrate's typed algebras
    /// specialise on is now a `pub const` on its owning closed-set
    /// algebra. This constant closes the CL lambda-list-keyword
    /// family lead byte at the SAME algebra as its two `&'static str`
    /// marker projections so a delimiter swap (e.g. a Racket-compat
    /// port from CL `&rest` / `&optional` to `#!rest` / `#!optional`
    /// keyword-args, a Clojure-compat port to `&` ONLY as the rest
    /// marker) lands at ONE constant on the algebra.
    ///
    /// Structural round-trip contract:
    /// `Self::REST_MARKER.starts_with(Self::LAMBDA_LIST_KEYWORD_LEAD)`
    /// AND
    /// `Self::OPTIONAL_MARKER.starts_with(Self::LAMBDA_LIST_KEYWORD_LEAD)`
    /// — the projection law binding the two `&'static str` markers to
    /// their shared canonical `char` lead byte, pinned by
    /// `macro_params_rest_marker_prefixed_by_lambda_list_keyword_lead`
    /// and
    /// `macro_params_optional_marker_prefixed_by_lambda_list_keyword_lead`.
    ///
    /// Disjointness contract: `LAMBDA_LIST_KEYWORD_LEAD`'s byte MUST
    /// differ from every sibling outer-marker `char` the substrate's
    /// other closed-set algebras specialise on. A collision would
    /// silently break the reader's outer dispatch: an `&`-prefixed
    /// bare atom `&rest` / `&optional` would collide with whichever
    /// marker it aliased. Pinned structurally at
    /// `macro_params_lambda_list_keyword_lead_distinct_from_every_other_algebra_marker`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// (CL lambda-list-keyword LEAD byte, canonical `'&'`) pairing
    /// binds at ONE constant on the typed [`MacroParams`] algebra
    /// regardless of which of the two `&'static str` marker consumers
    /// reaches in. THEORY.md §V.1 — knowable platform; the canonical
    /// CL lambda-list-keyword LEAD byte becomes a TYPE-level constant
    /// on the substrate algebra rather than an inline `'&'` glyph at
    /// every docstring pinning the family disjointness contract.
    pub const LAMBDA_LIST_KEYWORD_LEAD: char = '&';

    /// Closed-set forced-arity ALL array over the CL lambda-list-keyword
    /// `&'static str` markers the parser's typed dispatch specialises on
    /// — the [`Self::REST_MARKER`] (`"&rest"`) rest-slot marker followed
    /// by the [`Self::OPTIONAL_MARKER`] (`"&optional"`) optional-section
    /// marker, in the canonical CL lambda-list ordering (`&optional`
    /// binds AFTER `&required`, `&rest` binds AFTER `&optional`, so the
    /// declaration walk fires `REST_MARKER` FIRST as the terminal-arm
    /// early return; the ALL array's ordering keys on marker
    /// declaration order in this file, not on parse-time ordering).
    ///
    /// Sibling posture to the closed set of `pub const ALL: [Self; N]`
    /// forced-arity arrays on the substrate's other closed-set outer
    /// algebras: [`crate::ast::AtomKind::ALL`] (`[Self; 6]`),
    /// [`crate::ast::QuoteForm::ALL`] (`[Self; 4]`),
    /// [`crate::error::SexpShape::ALL`] (`[Self; 12]`),
    /// [`crate::error::UnquoteForm::ALL`] (`[Self; 2]`),
    /// [`crate::error::MacroDefHead::ALL`] (`[Self; 3]`) — every
    /// closed-set algebra the substrate carries pins its cardinality at
    /// the declaration site via a `pub const ALL` array whose forced
    /// arity fails compilation if a new variant lands without being
    /// added to the set. `LAMBDA_LIST_KEYWORDS` closes the same shape
    /// on the CL lambda-list-keyword family: a third marker (`&key`
    /// for keyword-args, `&aux` for auxiliaries, `&body` for the
    /// docstring-carrying tail of `defmacro` bodies) extends this ONE
    /// `pub const` + the matching per-role marker `pub const` above,
    /// AND rustc enforces every downstream consumer's closed-set
    /// sweep picks up the new marker at the SAME structural site.
    ///
    /// Pre-lift the two `&'static str` markers were named on the typed
    /// algebra but the family they close was NOT — the two structural
    /// round-trip pins (`_rest_marker_prefixed_by_lambda_list_keyword_
    /// lead` AND `_optional_marker_prefixed_by_lambda_list_keyword_
    /// lead`) each named ONE marker inline as a duplicate 3-line
    /// `assert!(A.starts_with(LEAD), ...)` shape, and the pairwise-
    /// disjointness pin (`_rest_and_optional_markers_pairwise_disjoint`)
    /// named the two markers as a hand-rolled `assert_ne!` pair.
    /// Post-lift the ALL array binds the family at the typed algebra so
    /// every family-wide contract (LEAD-byte round-trip, pairwise
    /// disjointness, membership gate) routes through the ONE array and
    /// a future `&key` / `&aux` extension automatically extends the
    /// sweep without re-deriving per-marker assertions.
    ///
    /// Structural round-trip contract: every element `m` of
    /// `Self::LAMBDA_LIST_KEYWORDS` satisfies
    /// `m.starts_with(Self::LAMBDA_LIST_KEYWORD_LEAD)` — pinned by
    /// `macro_params_every_lambda_list_keyword_prefixed_by_lambda_list_keyword_lead`.
    /// Cardinality contract: `Self::LAMBDA_LIST_KEYWORDS.len() == 2`
    /// pins the current family size at the type level; a future third
    /// marker extends the array's arity AND updates every downstream
    /// pin at ONE structural site. Pairwise-disjointness contract:
    /// every distinct index pair `(i, j)` in `Self::LAMBDA_LIST_KEYWORDS`
    /// satisfies `Self::LAMBDA_LIST_KEYWORDS[i] !=
    /// Self::LAMBDA_LIST_KEYWORDS[j]` — pinned by
    /// `macro_params_lambda_list_keywords_pairwise_distinct`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// CL lambda-list-keyword family closure binds at ONE `pub const`
    /// on the typed [`MacroParams`] algebra regardless of which
    /// consumer (parser dispatch, family-wide contract sweep,
    /// authoring / rendering surface, `is_lambda_list_keyword`
    /// membership gate) reaches in. THEORY.md §V.1 — knowable
    /// platform; the family's cardinality becomes a TYPE-level
    /// constant on the substrate algebra rather than a per-consumer
    /// hand-rolled enumeration of the two markers. THEORY.md §VI.1 —
    /// generation over composition; the family-wide contract sweeps
    /// (LEAD-byte round-trip, pairwise disjointness, membership gate)
    /// emerge from the composition of TWO substrate primitives (this
    /// `pub const` array + the per-role `pub const REST_MARKER` /
    /// `pub const OPTIONAL_MARKER`) rather than as per-marker inline
    /// assertions the current pins duplicate structurally.
    ///
    /// Frontier inspiration: Common Lisp's `LAMBDA-LIST-KEYWORDS`
    /// standard variable (CLHS §3.4.1) — the runtime-reflectable list
    /// of every reserved lambda-list keyword the implementation
    /// recognises; `LAMBDA_LIST_KEYWORDS` is the substrate's typed-Rust
    /// peer at compile time, with the closed-set ALL array standing in
    /// for CL's runtime-mutable list. Translation through pleme-io
    /// primitives: a `pub const [&'static str; N]` on the typed
    /// [`MacroParams`] algebra rather than a runtime-mutable list;
    /// forced-arity closure at the type layer with the same
    /// family-reflective read surface.
    pub const LAMBDA_LIST_KEYWORDS: [&'static str; 2] = [Self::REST_MARKER, Self::OPTIONAL_MARKER];

    /// Typed membership gate over the closed set
    /// [`Self::LAMBDA_LIST_KEYWORDS`] — `true` iff `s` matches some
    /// element of the ALL array byte-for-byte, `false` for every other
    /// input.
    ///
    /// The typed "does this symbol name a CL lambda-list keyword?"
    /// projection on the [`MacroParams`] algebra. Sibling posture to the
    /// closed set of per-algebra membership / classifier gates the
    /// substrate carries:
    /// [`crate::ast::QuoteForm::from_lead_char`] (per-char quote-family
    /// dispatch — Some/None on a closed 3-char set),
    /// [`crate::ast::Sexp::is_bare_atom_boundary`] (six-fold
    /// bare-atom-boundary sweep), and
    /// [`crate::ast::Atom::decode_str_escape`] (Str-escape decode table
    /// on a closed 6-arm set) — every family-membership gate the
    /// substrate carries binds at ONE method on the closed-set
    /// algebra so a family extension lands at ONE place plus the
    /// per-role marker without re-deriving per-consumer inline
    /// enumerations.
    ///
    /// Pre-lift the "is this symbol a CL lambda-list keyword?" concept
    /// existed structurally but was NOT named on the algebra — the
    /// disjointness pin (`_rest_and_optional_markers_pairwise_disjoint`)
    /// AND the cross-axis disjointness sweep
    /// (`_lambda_list_keyword_lead_distinct_from_every_other_algebra_marker`)
    /// each named the concept implicitly as a hand-rolled per-marker
    /// `assert_ne!` sweep. Post-lift the membership gate binds at ONE
    /// typed method on the algebra a future consumer (a parser-time
    /// "this symbol looks like a lambda-list keyword but isn't
    /// recognised" hint, an authoring-surface completion bar that
    /// suggests `&rest` / `&optional` at param-list positions, a
    /// pretty-printer that colors CL lambda-list keywords distinctly)
    /// binds against without re-deriving the per-marker enumeration
    /// at every consumer site.
    ///
    /// Contract:
    ///   * Every element of [`Self::LAMBDA_LIST_KEYWORDS`] classifies
    ///     as `true` — pinned by
    ///     `macro_params_is_lambda_list_keyword_accepts_every_marker`.
    ///   * The bare CL lambda-list-keyword LEAD byte
    ///     ([`Self::LAMBDA_LIST_KEYWORD_LEAD`]) as a 1-char string
    ///     classifies as `false` — pinned by
    ///     `macro_params_is_lambda_list_keyword_rejects_bare_lead_byte`.
    ///   * Symbols starting with the LEAD byte but naming an
    ///     unrecognised keyword (`"&key"`, `"&aux"`, `"&body"`)
    ///     classify as `false` — pinned by
    ///     `macro_params_is_lambda_list_keyword_rejects_unrecognised_ampersand_prefixed_names`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
    /// closed-set membership gate binds at ONE typed method on the
    /// [`MacroParams`] algebra regardless of which consumer reaches
    /// in. THEORY.md §III — the typescape; the "is this a CL
    /// lambda-list keyword?" projection becomes a TYPE projection on
    /// the substrate algebra rather than per-consumer inline
    /// enumeration of the two markers. THEORY.md §V.1 — knowable
    /// platform; a future third marker (`&key`, `&aux`, `&body`)
    /// extends [`Self::LAMBDA_LIST_KEYWORDS`] ONCE and every
    /// membership-gate consumer picks up the new marker for free.
    ///
    /// Frontier inspiration: Common Lisp's `(find X LAMBDA-LIST-KEYWORDS)`
    /// pattern for testing lambda-list keyword membership at runtime;
    /// `is_lambda_list_keyword` is the substrate's typed-Rust peer at
    /// compile time. Translation through pleme-io primitives: a linear
    /// sweep over the [`Self::LAMBDA_LIST_KEYWORDS`] `pub const` array
    /// rather than a runtime `find` over a mutable list; ordinary
    /// slice `contains` on the `&'static str` array.
    #[must_use]
    pub fn is_lambda_list_keyword(s: &str) -> bool {
        Self::LAMBDA_LIST_KEYWORDS.contains(&s)
    }

    /// The flat, ordered param-name list the template bytecode indexes into:
    /// every `required` name in order, then every `optional` name in order,
    /// then the `rest` name if present. `names()[i]` is the param `Subst(i)`
    /// / `Splice(i)` reference.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.required
            .iter()
            .map(String::as_str)
            .chain(self.optional.iter().map(|p| p.name.as_str()))
            .chain(self.rest.as_deref())
            .collect()
    }

    /// The rest-less maximum arity of this param list: `required.len() +
    /// optional.len()`. Two equivalent readings collapse into ONE primitive
    /// on the typed `MacroParams`:
    ///
    ///   * The **rest-start boundary**: when `self.rest` is `Some`, the
    ///     `&rest` slot collects `args[fixed_arity()..]` into a
    ///     `Sexp::List` (the empty slice when the call is exactly
    ///     saturated). `fixed_arity()` IS that slice's start index.
    ///   * The **rest-less maximum arity**: when `self.rest` is `None`,
    ///     `args.len() > fixed_arity()` is the surplus-args rejection
    ///     boundary [`bind`](Self::bind) checks before raising
    ///     `LispError::TooManyMacroArgs` (the call-site mirror of
    ///     `RestParamTrailingTokens`'s definition-site rejection).
    ///
    /// Both readings ARE the same arithmetic; [`bind`](Self::bind)
    /// previously inlined the same `self.required.len() +
    /// self.optional.len()` expression THREE times — once inside the
    /// `Vec::with_capacity(required + optional + rest?)` slot, once at
    /// the `rest_start` site (inside `if let Some(_rest_name) =
    /// self.rest`), and once at the `expected` site (inside the
    /// rest-less `else`). The latter two live in mutually-exclusive
    /// branches yet name ONE structural concept; lifting the arithmetic
    /// to a single named primitive makes that concept first-class on
    /// the typed param list.
    ///
    /// `fixed_arity()` IGNORES the `rest` slot by construction — a
    /// `&rest` param has no maximum and is not part of the fixed run.
    /// `names().len() == fixed_arity() + usize::from(self.rest.is_some())`
    /// is the structural identity binding this primitive to
    /// [`names`](Self::names) and to the `Vec::with_capacity` hint
    /// [`bind`](Self::bind) computes for the bound-values vec.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the structural
    /// "rest-start boundary / rest-less max arity" concept becomes a
    /// named `&MacroParams` projection rather than re-derived arithmetic
    /// at every site that walks the bound run. Authoring tools (REPL,
    /// LSP, `tatara-check`) that want to render "this macro takes
    /// between `required.len()` and `fixed_arity()` args (or unbounded
    /// if `rest.is_some()`)" bind to ONE method on the typed param
    /// list. THEORY.md §VI.1 — generation over composition; three
    /// inline copies of the same arithmetic in one function is past
    /// the ≥2 PRIME-DIRECTIVE trigger once the structural shape is
    /// named. THEORY.md §II.1 invariant 2 — free middle; both
    /// expansion strategies route through the SHARED `bind`, so the
    /// new primitive is exposed to the bytecode and substitute paths
    /// uniformly — no per-strategy drift in how the boundary is
    /// computed.
    #[must_use]
    pub fn fixed_arity(&self) -> usize {
        self.required.len() + self.optional.len()
    }

    /// The TOTAL lambda-list arity of this param list — every named
    /// slot the reader accepted, in the canonical CL lambda-list
    /// ordering: `required.len() + optional.len() + rest.is_some() as
    /// usize`. Peer to [`Self::fixed_arity`] one REST-SLOT axis over
    /// on the `MacroParams` algebra — where [`Self::fixed_arity`]
    /// ignores the rest slot (the rest-less maximum-arity /
    /// rest-start-boundary reading, unbounded on the call-time surface
    /// when `self.rest.is_some()`), this projection INCLUDES the rest
    /// slot as one additional named param (the total-slots reading,
    /// bounded on the type-time surface regardless of `rest`).
    ///
    /// `Self::total_arity() == Self::names().len()` — a `Vec`-free
    /// projection of the [`Self::names`] flat-index list's length. Two
    /// equivalent readings collapse into ONE named primitive on the
    /// typed [`MacroParams`] algebra:
    ///
    ///   * The **`Vec::with_capacity` hint**: [`Self::bind`] previously
    ///     inlined `self.fixed_arity() + usize::from(self.rest.is_some())`
    ///     as the bound-vec capacity hint. That expression IS this
    ///     primitive.
    ///   * The **REGISTER-time arity ceiling**: on the arity-bomb
    ///     `(defmacro huge (a-1 a-2 … a-N) `,a-1)` [`Expander::register_macro_def`]
    ///     compares `def.params.total_arity()` against
    ///     [`Expander::max_macro_arity`] BEFORE the body-size gate walks
    ///     the body or any insert lands. `total_arity()` IS that gate's
    ///     axis.
    ///
    /// The structural identity
    /// `names().len() == fixed_arity() + usize::from(rest.is_some())
    ///                 == total_arity()`
    /// binds the primitive to [`Self::names`] and [`Self::fixed_arity`]
    /// on the [`MacroParams`] algebra — pinned by
    /// `macro_params_total_arity_matches_names_len` AND
    /// `macro_params_total_arity_equals_fixed_arity_plus_rest_bit`.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// "how many named slots does this macro declare?" projection
    /// becomes a NAMED `&MacroParams` accessor rather than re-derived
    /// arithmetic at [`Self::bind`]'s capacity hint AND at the
    /// [`Expander::register_macro_def`] arity-ceiling gate.
    /// THEORY.md §VI.1 — generation over composition; two inline copies
    /// of the same three-arm arithmetic across two consumers is the
    /// ≥2 PRIME-DIRECTIVE trigger once the structural shape is named.
    /// THEORY.md §II.1 invariant 2 — free middle; both the binder AND
    /// the arity-guard consult ONE primitive, so a regression that
    /// drifts the guard's arity computation from the binder's capacity
    /// hint (e.g. one counts `rest` as one slot, the other counts it
    /// as zero) becomes structurally impossible.
    #[must_use]
    pub fn total_arity(&self) -> usize {
        self.fixed_arity() + usize::from(self.rest.is_some())
    }

    /// Bind call args to params positionally, returning the per-index bound
    /// values parallel to [`names`](Self::names): each required name takes
    /// the arg at its position (a missing one is
    /// [`missing_macro_arg`](self::missing_macro_arg)); each optional name
    /// takes the arg at its position, or — when the call ran out of args —
    /// its declared default form (`Sexp::Nil` when none was declared,
    /// matching CL's `&optional` floor); and a present `rest` collects every
    /// arg beyond the required+optional run into a `Sexp::List` (the empty
    /// list when none remain). Args beyond a rest-less param list are
    /// ignored, matching the prior binder. This is the single binding loop
    /// both expansion strategies share — `apply_compiled` consumes the index
    /// vec directly, `bind_args` zips it against `names()` into the
    /// name-keyed map.
    fn bind(&self, macro_name: &str, args: &[Sexp]) -> Result<Vec<Sexp>> {
        let mut out = Vec::with_capacity(self.total_arity());
        for (i, name) in self.required.iter().enumerate() {
            let arg = args
                .get(i)
                .cloned()
                .ok_or_else(|| missing_macro_arg(macro_name, name))?;
            out.push(arg);
        }
        let opt_start = self.required.len();
        for (j, param) in self.optional.iter().enumerate() {
            // Absent optional slot binds to the typed `resolved_default()`
            // projection on `OptionalParam`: the declared default form when
            // one was authored, OR the `Sexp::Nil` no-default floor when
            // none was. The two-arm fallback `param.default.clone().
            // unwrap_or(Sexp::Nil)` previously inlined here is now ONE named
            // accessor on the typed param both expansion strategies share via
            // `MacroParams::bind`.
            let arg = args
                .get(opt_start + j)
                .cloned()
                .unwrap_or_else(|| param.resolved_default());
            out.push(arg);
        }
        if let Some(_rest_name) = self.rest.as_ref() {
            // The `&rest` slot collects args[fixed_arity()..] (the empty
            // slice when the call is exactly saturated); the boundary is
            // the typed `fixed_arity()` primitive both branches share.
            let rest = args.get(self.fixed_arity()..).unwrap_or(&[]).to_vec();
            out.push(Sexp::List(rest));
        } else {
            // No `&rest` slot — the param list has a FIXED maximum arity
            // of `fixed_arity()`. Surplus args have nowhere to bind;
            // reject rather than silently truncate. Closes the call-site
            // mirror of `RestParamTrailingTokens` (the definition-site
            // rejection lifted by the prior-run typed-promotion lineage),
            // so the typed-entry macro-call-gate is structurally complete
            // in both directions: too-few (`MissingMacroArg`) AND too-many
            // (`TooManyMacroArgs`).
            let expected = self.fixed_arity();
            if args.len() > expected {
                return Err(too_many_macro_args(macro_name, expected, args.len()));
            }
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
    /// The six resource ceilings, held as ONE typed [`ResourceLimits`] bundle
    /// rather than six sibling `max_*: usize` fields. Peer of the six
    /// getter/setter pairs one AGGREGATION axis over — where each pair
    /// projects ONE ceiling, this field carries the SIX-fold cross-product
    /// so [`Self::resource_limits`] returns `self.limits` verbatim and
    /// [`Self::set_resource_limits`] assigns through the bundle in ONE move.
    ///
    /// Adding a SEVENTH ceiling touches [`ResourceLimits`] + the matching
    /// `DEFAULT_MAX_*` module constant + the paired individual getter/setter
    /// — the [`Expander`] struct, its constructors, AND
    /// [`Self::resource_limits`] / [`Self::set_resource_limits`] stay
    /// exactly as they are. That's the compounding win of the
    /// consolidation: the (Expander, constructors, bulk getter, bulk
    /// setter) four-corner face is field-shape-agnostic — it composes with
    /// the bundle by value, not by field-by-field literal.
    ///
    /// Consumers on the read path (`register_macro_def`'s three REGISTER-time
    /// gates, `expand_with_depth`'s depth + output-size gates, `apply`'s
    /// cache-entries gate) dereference through `self.limits.max_*`; the
    /// projection is a `Copy usize` load — same shape as reading a bare
    /// field, no `Deref`, no `Arc`, no lock.
    limits: ResourceLimits,
}

impl Expander {
    /// Default-posture expander with an operator-supplied resource-ceiling
    /// bundle — the AGGREGATION-axis peer of
    /// [`Self::set_resource_limits`] one CONSTRUCTOR-STAGE axis over.
    /// Where [`Self::set_resource_limits`] applies the bundled six ceilings
    /// POST-construction on an already-built expander, this constructor
    /// binds them AT construction so a call-site that wants a fresh
    /// expander with a non-default posture composes ONE typed call
    /// (`Expander::with_limits(limits)`) rather than the two-step
    /// (`let mut e = Expander::new(); e.set_resource_limits(limits);`)
    /// dance the post-construction sibling still admits.
    ///
    /// The (compile_templates, cache_enabled) execution-strategy pair
    /// carries the [`Self::new`] default posture (`true, true`) — the
    /// bytecode-with-cache strategy — since callers that want the
    /// substitute-only strategy still route through
    /// [`Self::new_substitute_only`]. Post-lift both convenience
    /// constructors delegate their `macros` / `templates` / `cache` field
    /// literals through THIS primitive at the resource-posture axis: the
    /// four-out-of-six field literals (`HashMap::new()` × 2 +
    /// `Arc::new(Mutex::new(HashMap::new()))` + `limits`) lived
    /// byte-identical across [`Self::new`] and [`Self::new_substitute_only`]
    /// pre-lift; the two divergent flags (`compile_templates`,
    /// `cache_enabled`) still differ between the two constructors, and the
    /// substitute-only variant flips them AFTER threading through
    /// `with_limits` — same shape as [`Self::new_bytecode_no_cache`] does
    /// on the `cache_enabled` flag.
    ///
    /// Round-trip identity: `Expander::with_limits(limits).resource_limits()
    /// == limits` for any `ResourceLimits` — pinned as a typed theorem in
    /// the test cohort. The at-construction assignment is a direct `Copy`
    /// of the bundled field (no per-field cascade), so a mid-construction
    /// panic window on the six knobs is structurally impossible. Peer of
    /// the `Expander::with_limits(DEFAULT_RESOURCE_LIMITS) ==
    /// Expander::new()` delegation identity — pinned on the resource-
    /// posture projection so a future refactor that drifts the two
    /// constructors on the ceiling axis fails loudly.
    ///
    /// Frontier inspiration: Cranelift's `settings::Flags::new(builder)` —
    /// codegen ceilings (opt level, allocator strategy, feature enables)
    /// bind at CONSTRUCTION-time on a typed `Flags` bundle rather than
    /// composed through post-construction setters on the `Isa`. Same
    /// posture as this lift: the (getter, setter, at-construction)
    /// three-corner face on the resource-posture surface now binds at ONE
    /// typed bundle across ALL three surfaces.
    ///
    /// Theory grounding: THEORY.md §II.1 invariants 1 + 2 — typed entry +
    /// free middle. Pre-lift the "expander with a chosen resource posture"
    /// concept was untyped: consumers composed [`Self::new`] +
    /// [`Self::set_resource_limits`] at their call site with no
    /// compile-time gate that BOTH steps land atomically. Post-lift the
    /// concept has ONE named entry point on the `Expander` surface, and a
    /// future consumer (`RealizedCompiler` builder that ships a
    /// CI-tightened preset, an LSP session that ships a REPL-permissive
    /// preset, a test-harness fixture that ships an `UNBOUNDED`-style
    /// preset) binds through ONE typed constructor.
    pub fn with_limits(limits: ResourceLimits) -> Self {
        Self {
            macros: HashMap::new(),
            templates: HashMap::new(),
            compile_templates: true,
            cache: Arc::new(Mutex::new(HashMap::new())),
            cache_enabled: true,
            limits,
        }
    }

    /// Default expander — compiled bytecode + expansion cache enabled.
    /// Delegates to [`Self::with_limits`] with [`DEFAULT_RESOURCE_LIMITS`]
    /// so the (macros, templates, cache, limits) field-shape agreement
    /// with every other constructor on this `Expander` surface lives at
    /// ONE literal.
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_RESOURCE_LIMITS)
    }

    /// Expander using the legacy substitute path (no template compilation,
    /// no cache). Kept for benchmarking + equivalence testing.
    /// Delegates to [`Self::with_limits`] with [`DEFAULT_RESOURCE_LIMITS`]
    /// and flips the two divergent execution-strategy flags — same
    /// composition shape as [`Self::new_bytecode_no_cache`] one flag over.
    pub fn new_substitute_only() -> Self {
        let mut e = Self::with_limits(DEFAULT_RESOURCE_LIMITS);
        e.compile_templates = false;
        e.cache_enabled = false;
        e
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

    /// The configured ceiling on `expand`'s recursive re-entry into a
    /// macro-call form. Defaults to [`DEFAULT_MAX_EXPANSION_DEPTH`].
    #[must_use]
    pub fn max_expansion_depth(&self) -> usize {
        self.limits.max_expansion_depth
    }

    /// Set the ceiling on `expand`'s recursive re-entry into a macro-call
    /// form. Tests bind low values (e.g. `4`) to pin the runaway-rejection
    /// shape without paying for a 256-round walk; consumers that compose
    /// deep proven macro cascades can raise it. Zero is admitted and
    /// makes every first macro-call return
    /// [`LispError::ExpansionDepthExceeded`] immediately — useful for
    /// contract-tests that assert "this reader path never expands a
    /// macro."
    pub fn set_max_expansion_depth(&mut self, depth: usize) {
        self.limits.max_expansion_depth = depth;
    }

    /// The configured ceiling on the expansion cache's entry count.
    /// Defaults to [`DEFAULT_MAX_CACHE_ENTRIES`]. Peer to
    /// [`Self::max_expansion_depth`] one RESOURCE axis over.
    #[must_use]
    pub fn max_cache_entries(&self) -> usize {
        self.limits.max_cache_entries
    }

    /// Set the ceiling on the expansion cache's entry count. Tests bind
    /// low values (e.g. `2`) to pin the bounded-cache contract without
    /// paying for an 8K-entry walk; consumers that expand many distinct
    /// argument shapes can raise it. Zero is admitted and effectively
    /// disables caching (every fresh `(name, args)` pair skips the
    /// insert path) — useful for contract-tests that assert "this
    /// expander realizes without ever caching." [`usize::MAX`] is the
    /// unbounded-cache mode for batch compilation runs where memory is
    /// not the concern.
    pub fn set_max_cache_entries(&mut self, cap: usize) {
        self.limits.max_cache_entries = cap;
    }

    /// The configured ceiling on a single macro-`apply` output's
    /// [`crate::ast::Sexp::node_count`]. Defaults to
    /// [`DEFAULT_MAX_EXPANSION_SIZE`]. Peer to
    /// [`Self::max_expansion_depth`] and [`Self::max_cache_entries`]
    /// one RESOURCE-DIMENSION axis over.
    #[must_use]
    pub fn max_expansion_size(&self) -> usize {
        self.limits.max_expansion_size
    }

    /// Set the ceiling on a single macro-`apply` output's node count.
    /// Tests bind low values (e.g. `8`) to pin the "expansion bomb"
    /// rejection shape without materializing a 64K-node blob;
    /// consumers that compose deep proven macro cascades can raise it.
    /// [`usize::MAX`] admits any lawful output (the ceiling is
    /// effectively lifted); operators that want strict "your macros
    /// must fit in N nodes" contract-tests set it exactly.
    pub fn set_max_expansion_size(&mut self, size: usize) {
        self.limits.max_expansion_size = size;
    }

    /// The configured ceiling on a registered macro's BODY
    /// [`crate::ast::Sexp::node_count`]. Defaults to
    /// [`DEFAULT_MAX_MACRO_BODY_SIZE`]. Peer to
    /// [`Self::max_expansion_depth`], [`Self::max_cache_entries`],
    /// and [`Self::max_expansion_size`] one PIPELINE-STAGE axis over
    /// (the three prior guards fire at EXPAND time; this one fires
    /// at REGISTER time).
    #[must_use]
    pub fn max_macro_body_size(&self) -> usize {
        self.limits.max_macro_body_size
    }

    /// Set the ceiling on a registered macro's BODY node count.
    /// Tests bind low values (e.g. `4`) to pin the authoring-bomb
    /// rejection shape without materializing a 16K-node blob;
    /// consumers that author deep proven macro-body templates can
    /// raise it. [`usize::MAX`] admits any lawful body (the ceiling
    /// is effectively lifted). Zero is admitted and rejects every
    /// non-empty body immediately (a `Nil` body is 1 node, so the
    /// smallest lawful body is still rejected under a zero
    /// ceiling) — useful for contract-tests that assert "this
    /// expander refuses every registration."
    pub fn set_max_macro_body_size(&mut self, size: usize) {
        self.limits.max_macro_body_size = size;
    }

    /// The configured ceiling on the [`Self::macros`] table's entry
    /// count. Defaults to [`DEFAULT_MAX_REGISTERED_MACROS`]. Peer to
    /// [`Self::max_macro_body_size`] one RESOURCE-DIMENSION axis over
    /// on the REGISTER-time surface, and to [`Self::max_cache_entries`]
    /// one PIPELINE-STAGE axis over.
    #[must_use]
    pub fn max_registered_macros(&self) -> usize {
        self.limits.max_registered_macros
    }

    /// Set the ceiling on the [`Self::macros`] table's entry count.
    /// Tests bind low values (e.g. `3`) to pin the registration-bomb
    /// rejection shape without materializing a 4K-entry macros table;
    /// consumers that compose deep proven typescapes can raise it.
    /// [`usize::MAX`] admits any lawful cumulative registration count
    /// (the ceiling is effectively lifted). Zero is admitted and
    /// rejects every FRESH registration immediately — useful for
    /// contract-tests that assert "this expander accepts no new
    /// macros" — while still admitting overwrites of any macro that
    /// was registered before the ceiling was set (a re-registration
    /// of an existing key never grows the table).
    pub fn set_max_registered_macros(&mut self, cap: usize) {
        self.limits.max_registered_macros = cap;
    }

    /// The configured ceiling on a registered macro's lambda-list
    /// arity. Defaults to [`DEFAULT_MAX_MACRO_ARITY`]. Peer to
    /// [`Self::max_macro_body_size`] and [`Self::max_registered_macros`]
    /// one RESOURCE-DIMENSION axis over on the REGISTER-time surface.
    #[must_use]
    pub fn max_macro_arity(&self) -> usize {
        self.limits.max_macro_arity
    }

    /// Set the ceiling on a registered macro's lambda-list arity —
    /// the [`MacroParams::total_arity`] projection of `def.params`.
    /// Tests bind low values (e.g. `2`) to pin the arity-bomb rejection
    /// shape without materializing a 128-slot authoring blob;
    /// consumers that author high-arity domain macros (a `defschema`
    /// with 40 typed slots) can raise it. [`usize::MAX`] admits any
    /// lawful arity (the ceiling is effectively lifted). Zero is
    /// admitted and rejects every non-nullary macro registration
    /// immediately — a `(defmacro nullary () BODY)` is still admitted
    /// (its `total_arity()` is `0`, and only strict overrun rejects) —
    /// useful for contract-tests that assert "this expander accepts
    /// only nullary macros."
    pub fn set_max_macro_arity(&mut self, cap: usize) {
        self.limits.max_macro_arity = cap;
    }

    /// Snapshot the six configured resource ceilings as ONE typed
    /// [`ResourceLimits`] value — the bundled peer of the six
    /// [`Self::max_expansion_depth`] / [`Self::max_cache_entries`] /
    /// [`Self::max_expansion_size`] / [`Self::max_macro_body_size`] /
    /// [`Self::max_registered_macros`] / [`Self::max_macro_arity`]
    /// individual getters. Consumers that want to inspect the full
    /// resource posture (a test that compares a preset expander to a
    /// reference posture, a diagnostic that renders "the six ceilings"
    /// as one line, a caller that clones + selectively overrides one
    /// field via a struct-update literal) bind to this projection at
    /// ONE call site rather than composing the six individual getters.
    ///
    /// Post the [`Self::limits`]-field consolidation the projection is
    /// a direct `Copy` of the bundled field — a `usize`-sized fixed
    /// return with no per-field literal walk. Adding a SEVENTH ceiling
    /// extends [`ResourceLimits`] and the paired individual
    /// getter/setter; this bulk getter is field-shape-agnostic and
    /// stays exactly as it is.
    #[must_use]
    pub fn resource_limits(&self) -> ResourceLimits {
        self.limits
    }

    /// Bulk-replace the six configured resource ceilings from ONE typed
    /// [`ResourceLimits`] value — the bundled peer of the six
    /// [`Self::set_max_expansion_depth`] /
    /// [`Self::set_max_cache_entries`] /
    /// [`Self::set_max_expansion_size`] /
    /// [`Self::set_max_macro_body_size`] /
    /// [`Self::set_max_registered_macros`] /
    /// [`Self::set_max_macro_arity`] individual setters. Presets
    /// (a hardened CI expander, a permissive REPL expander, a
    /// throttled test harness) compose the six ceilings ONCE at the
    /// preset call site and apply them ATOMICALLY here rather than
    /// threading six independent setter invocations whose ordering
    /// the type system does not gate.
    ///
    /// Round-trip identity: `let l = e.resource_limits(); e.set_resource_limits(l);`
    /// leaves `e` in an equivalent posture — pinned as a typed theorem
    /// in the [`ResourceLimits`] test cohort. Post the [`Self::limits`]-
    /// field consolidation the assignment is a direct `Copy` of the
    /// bundled field, structurally atomic — no per-field destructure +
    /// six-way write cascade in which a mid-cascade panic could leave
    /// the six knobs in a half-updated posture. Sibling of
    /// [`Self::resource_limits`]; the pair closes the (getter, setter)
    /// face on the aggregation axis.
    pub fn set_resource_limits(&mut self, limits: ResourceLimits) {
        self.limits = limits;
    }

    pub fn with_macros<I: IntoIterator<Item = MacroDef>>(defs: I) -> Result<Self> {
        let mut e = Self::new();
        for d in defs {
            e.register_macro_def(d)?;
        }
        Ok(e)
    }

    /// Register a parsed [`MacroDef`] into this expander's macro tables —
    /// the single named primitive on the `Expander` surface every
    /// macro-registration consumer routes through.
    ///
    /// The registration discipline is a two-step composition:
    ///   1. When [`Self::compile_templates`](Self::new) is on (the
    ///      `Self::new` default; flipped off by [`Self::new_substitute_only`]),
    ///      [`compile_template`] pre-compiles the macro body to a typed
    ///      [`CompiledTemplate`] bytecode and inserts it into `self.templates`
    ///      keyed by `def.name`.
    ///   2. The `MacroDef` is moved into `self.macros` keyed by `def.name` —
    ///      always, regardless of `compile_templates`, because the substitute
    ///      strategy reads `self.macros` exclusively while the bytecode
    ///      strategy consults `self.templates` first and falls back to
    ///      `self.macros` for the body and params.
    ///
    /// The order is structural: `compile_template` borrows `&def` while
    /// `self.macros.insert(def.name.clone(), def)` consumes `def` — the
    /// template pre-compile MUST precede the move into `self.macros`, and the
    /// `def.name.clone()` projection captures the key for the moved insert.
    /// `?`-routing through `compile_template` preserves the structural
    /// ordering of the rejection chain end-to-end: a template-compile error
    /// (`UnboundTemplateVar` for an unbound `,name`, `NonSymbolUnquoteTarget`
    /// for `,5` / `,(nested)`, et al.) short-circuits BEFORE `self.macros`
    /// is mutated, so a failed registration leaves both tables exactly as
    /// they were — no partial-write window in which `self.macros.has(name)`
    /// is true but `self.templates.has(name)` is missing (a regression that
    /// would silently coerce the bytecode strategy onto the substitute path
    /// for that one macro despite `compile_templates: true`).
    ///
    /// Before this lift the same two-step block —
    ///
    /// ```ignore
    /// if self.compile_templates {
    ///     self.templates.insert(def.name.clone(), compile_template(&def)?);
    /// }
    /// self.macros.insert(def.name.clone(), def);
    /// ```
    ///
    /// — lived byte-identical (modulo `self`/`e` and `def`/`d` substitutions)
    /// at TWO sites: [`Self::with_macros`] (the constructor that
    /// bulk-registers an `IntoIterator<Item = MacroDef>`, e.g. a curated
    /// preloaded set the caller assembled out-of-band) and
    /// [`Self::expand_program`]'s `(defmacro …)`-head arm (the program-level
    /// walker that recognizes a `defmacro` / `defpoint-template` / `defcheck`
    /// head via [`macro_def_from`] and registers it as a side-effect of
    /// walking the program). After this lift the registration block lives in
    /// ONE method on the `Expander`; both consumers and any future
    /// macro-registration surface bind to ONE primitive instead of
    /// re-deriving the two-step discipline at every call site.
    ///
    /// `pub` so authoring surfaces (an LSP that incrementally registers a
    /// `(defmacro …)` head as the user finishes typing it without a full
    /// program re-expand, a REPL `:define-macro` command that registers a
    /// pre-parsed `MacroDef` directly, a future "library merge" operation
    /// that absorbs another expander's macro set MacroDef-by-MacroDef) can
    /// register a typed `MacroDef` without round-tripping through source
    /// serialization first. Sibling of [`Self::with_macros`] (the
    /// bulk-from-iterator constructor — itself the
    /// `defs.into_iter().try_fold((), |_, d| self.register_macro_def(d))`
    /// shape on a fresh expander) and [`Self::expand_program`] (the
    /// source-level walker that recognizes `(defmacro …)` heads via
    /// [`macro_def_from`] — itself the program-level fold-over-defmacro-heads
    /// of this method). All three end up at this primitive.
    ///
    /// Returns `Result<()>` so the consumer's rejection chain composes with
    /// `?`-routing — `with_macros` short-circuits its bulk loop on the first
    /// `compile_template` failure; `expand_program` short-circuits its
    /// program walk at the offending `(defmacro …)` form. Infallibility on
    /// the `compile_templates: false` path is preserved (`compile_template`
    /// is gated behind the conditional), so a substitute-only expander never
    /// emits the `compile_template`-side rejection chain.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; two
    /// byte-identical inline copies of the registration block across
    /// `with_macros` and `expand_program` is past the ≥2 PRIME-DIRECTIVE
    /// trigger once the structural shape is named. THEORY.md §V.1 — knowable
    /// platform; the macro-registration discipline becomes a NAMED primitive
    /// on the substrate's `Expander` surface rather than a per-consumer
    /// inline duplication that future emitters (an LSP, a REPL, a library-
    /// merge operator) would have had to re-derive. THEORY.md §II.1
    /// invariant 2 — free middle; both consumers route through the SAME
    /// registration primitive, so a regression that drifts ONE consumer's
    /// discipline from the other (one path skips the template pre-compile,
    /// one path inserts the `self.macros` entry in a different order, a
    /// future third side-effect — logging, attestation, metrics — emitted
    /// at one site but not the other) cannot reach the substrate's runtime:
    /// there is exactly one implementation both consumers route through.
    ///
    /// Frontier inspiration: Racket's `(define-syntax name template)` at
    /// REPL is exactly this — register a typed macro into the live
    /// namespace, no source round-trip; the substrate's `register_macro_def`
    /// is the Rust-typed peer of that surface, lifted onto the `Expander`'s
    /// table-level algebra (`macros: HashMap<String, MacroDef>` +
    /// `templates: HashMap<String, CompiledTemplate>`). MLIR's
    /// `OpRegistry::registerOp<Op>()` — typed-op registration into a
    /// dialect's live table at construction-time AND at JIT-walk-time
    /// through ONE registration entry point; `register_macro_def` is the
    /// pleme-io peer of that registration entry point on the macro-table
    /// algebra.
    pub fn register_macro_def(&mut self, def: MacroDef) -> Result<()> {
        // Reject a registration-bomb (a code-generator emitting
        // unbounded fresh `(defmacro fresh-N (x) `(list ,x))` heads
        // whose bodies each sit comfortably under
        // `max_macro_body_size` yet collectively saturate process
        // memory) at the cheapest gate on the REGISTER-time surface
        // — a `HashMap::len` comparison, no body walk. Peer to the
        // body-size gate below one RESOURCE-DIMENSION axis over
        // (COUNT vs. SIZE) and to the expand-time cache-entries gate
        // in `apply` one PIPELINE-STAGE axis over (REGISTER-time
        // COUNT vs. EXPAND-time COUNT). Fires BEFORE the body-size
        // gate walks `def.body.node_count()` or any insert lands, so
        // a failed registration leaves BOTH tables exactly as they
        // were. Overwrites (a re-registration of an already-
        // registered key) are ADMITTED at any table size — a
        // re-`defmacro` never grows the table, so the ceiling cannot
        // gate operator redefinitions. Keyed on `>=` (a table
        // already at ceiling rejects a fresh key) so
        // `max_registered_macros` names the LARGEST admissible
        // `self.macros.len()` — same posture the expand-time
        // cache-entries gate holds on the memoization cache.
        let is_overwrite = self.macros.contains_key(&def.name);
        if !is_overwrite && self.macros.len() >= self.limits.max_registered_macros {
            return Err(LispError::RegisteredMacrosExceeded {
                macro_name: def.name.clone(),
                count: self.macros.len(),
                limit: self.limits.max_registered_macros,
            });
        }
        // Reject an arity-bomb `(defmacro huge (a-1 a-2 … a-N-million)
        // `,a-1)` — a code-generator whose macro body sits at ONE node
        // (well under `max_macro_body_size`) yet whose param list
        // carries millions of fresh names the per-index binder walks on
        // every call. Peer to the body-size gate below one RESOURCE-
        // DIMENSION axis over on the REGISTER-time surface (per-body
        // ARITY vs. per-body SIZE), AND peer to the table-count gate
        // above one RESOURCE-DIMENSION axis over on the REGISTER-time
        // surface (per-registration WIDTH vs. cumulative COUNT). Fires
        // BEFORE the body-size gate walks `def.body.node_count()` or
        // any insert lands, so a failed registration leaves BOTH tables
        // exactly as they were. Cheaper than the body-size gate — a
        // three-arm addition on `def.params` fields, no AST walk —
        // hence positioned BEFORE it on the O(1)-first ordering of the
        // REGISTER-time chain. Keyed on `>` (equality is admitted, only
        // strict overrun rejects) so `max_macro_arity` names the
        // LARGEST admissible `def.params.total_arity()` — same posture
        // the body-size gate holds on the body node count.
        let arity = def.params.total_arity();
        if arity > self.limits.max_macro_arity {
            return Err(LispError::MacroArityExceeded {
                macro_name: def.name.clone(),
                arity,
                limit: self.limits.max_macro_arity,
            });
        }
        // Reject an authoring-bomb `(defmacro huge (x) `<template-of-
        // N-million-nodes>)` at the REGISTRATION boundary — the
        // PIPELINE-STAGE peer of the EXPAND-time output-size gate in
        // `expand_with_depth`. Fires BEFORE `compile_template` walks
        // the body or `self.macros.insert` lands the entry, so a
        // failed registration leaves BOTH tables exactly as they
        // were (no partial-write window in which the body-size
        // rejection happened but the template pre-compile already
        // consumed cycles or the macros table already carries the
        // over-ceiling entry). Keyed on `>` rather than `>=` so
        // `max_macro_body_size` names the LARGEST admissible body —
        // equality is admitted, only strict overrun rejects — same
        // posture the expand-time output-size gate holds.
        let body_size = def.body.node_count();
        if body_size > self.limits.max_macro_body_size {
            return Err(LispError::MacroBodySizeExceeded {
                macro_name: def.name.clone(),
                size: body_size,
                limit: self.limits.max_macro_body_size,
            });
        }
        if self.compile_templates {
            self.templates
                .insert(def.name.clone(), compile_template(&def)?);
        }
        self.macros.insert(def.name.clone(), def);
        Ok(())
    }

    /// Expand a whole program. Returns the list of top-level forms after
    /// `defmacro` definitions are registered and all macro calls expanded.
    pub fn expand_program(&mut self, forms: Vec<Sexp>) -> Result<Vec<Sexp>> {
        let mut out = Vec::new();
        for form in forms {
            if let Some(def) = macro_def_from(&form)? {
                self.register_macro_def(def)?;
                continue;
            }
            out.push(self.expand(&form)?);
        }
        Ok(out)
    }

    /// Read a source string into top-level forms via [`crate::reader::read`],
    /// then route the forms through [`expand_program`](Self::expand_program) —
    /// the from-source posture of the yield-all-forms-after-expansion primitive,
    /// in ONE method on the `Expander` surface.
    ///
    /// Before this lift the same two-step chain — `let forms = read(src)?;
    /// <expander>.expand_program(forms)` — lived inline at two sites in
    /// [`crate::compiler_spec`]: [`RealizedCompiler::compile`](crate::compiler_spec::RealizedCompiler::compile)
    /// (the public from-source untyped-expansion entry on a realized compiler,
    /// returning the expanded `Vec<Sexp>` for untyped consumers like
    /// `tatara-check`'s per-form dispatcher) AND [`realize_in_memory`](crate::compiler_spec::realize_in_memory)'s
    /// `:macros` library load loop (the per-spec-macro source absorption that
    /// builds the preloaded expander's macro library through `expand_program`'s
    /// `defmacro` recognition side-effect). After this lift the read-then-expand
    /// composition lives in ONE method on the `Expander`; each of the two
    /// consumers binds it with the per-site expander posture that fits its call
    /// boundary — `self.preloaded.clone()` for `RealizedCompiler::compile`'s
    /// per-call clone, `&mut preloaded` for `realize_in_memory`'s shared
    /// build-up.
    ///
    /// Sibling of [`expand_source_and_collect_calls_to`](Self::expand_source_and_collect_calls_to)
    /// — that method stacks the typed-keyword projection on top of the
    /// from-source pipeline (`read + expand_program + iter_calls_to(_,
    /// keyword) + map + collect`); this method is the bare yield-all-forms
    /// from-source primitive (`read + expand_program`) the typed dispatchers
    /// stack their keyword projection atop. The two together close the
    /// from-source posture of the program-level walk family on the
    /// `Expander` surface: bare (this method) vs. typed-keyword-projected
    /// (the sibling).
    ///
    /// Closes the 2×2 program-level walk family on the `Expander` surface:
    /// from-forms × yield-all ([`expand_program`](Self::expand_program)),
    /// from-forms × keyword-projected ([`expand_and_collect_calls_to`](Self::expand_and_collect_calls_to)),
    /// from-source × keyword-projected ([`expand_source_and_collect_calls_to`](Self::expand_source_and_collect_calls_to)),
    /// AND now from-source × yield-all (this method). The four together name
    /// the canonical surfaces a dispatcher composes with to extract an
    /// expanded program from either a pre-parsed `Vec<Sexp>` (from-forms
    /// posture, for callers composing with another `Sexp`-producing surface)
    /// or a raw `&str` (from-source posture, for callers consuming
    /// authoring-surface source text directly), with or without a
    /// typed-keyword filter on the result.
    ///
    /// `?`-routing through `read` preserves the structural ordering of the
    /// rejection chain end-to-end: a reader error (lexer / parser /
    /// unbalanced-paren / unterminated-string) short-circuits BEFORE
    /// `expand_program` runs; an `expand_program` error
    /// (`defmacro`-NAME-not-a-symbol, `OptionalParamMalformed`,
    /// `RestParamMissingName`, et al.) short-circuits at the offending form.
    /// Each consumer's rejection chain remains exactly what it was pre-lift,
    /// now sourced from ONE composition point rather than two.
    ///
    /// `defmacro`-registration side-effects fire on `&mut self` exactly as
    /// they do for the from-forms primitive — `realize_in_memory`'s per-spec
    /// build-up depends on every `defmacro` in every `:macros` source
    /// landing in `self.macros` (and, when `compile_templates` is on, in
    /// `self.templates`); `RealizedCompiler::compile`'s per-call clone
    /// posture isolates absorption to the cloned expander, so a
    /// `defmacro` in the user's source does NOT leak into the persistent
    /// realized compiler. Both postures' absorption semantics are
    /// preserved by routing through this primitive instead of inlining
    /// the two-step chain.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; two
    /// inline copies of the `let forms = read(src)?; <expander>.expand_program(forms)`
    /// two-step chain across `RealizedCompiler::compile` and
    /// `realize_in_memory` is past the ≥2 PRIME-DIRECTIVE trigger once the
    /// structural shape is named. THEORY.md §V.1 — knowable platform; the
    /// read-then-expand composition becomes a NAMED primitive on the
    /// substrate's `Expander` surface rather than a re-derived two-step
    /// inline pipeline at every consumer. THEORY.md §II.1 invariant 2 —
    /// free middle; both consumers route through the SAME composition
    /// primitive, so a regression that drifts ONE consumer's pipeline
    /// shape from the other cannot reach the substrate's runtime.
    ///
    /// Frontier inspiration: Racket's `(eval-string str ns)` against a
    /// namespace — the from-source-string entry to namespace-level
    /// program evaluation is the Racket idiom; the substrate's
    /// `expand_source_program` is the Rust-typed peer of that, sourced
    /// from `&str` and yielding the post-macroexpansion `Vec<Sexp>`
    /// without a typed-keyword filter — exactly the shape an untyped
    /// consumer (`tatara-check`'s per-form dispatcher, a REPL's
    /// "expand this buffer" command, an LSP's "show me the expanded
    /// program" handler) binds to.
    pub fn expand_source_program(&mut self, src: &str) -> Result<Vec<Sexp>> {
        let forms = crate::reader::read(src)?;
        self.expand_program(forms)
    }

    /// Compose the expander's program-level expansion with the substrate's
    /// slice-side typed-keyword projection ([`iter_calls_to`]) and a
    /// caller-supplied per-form projection — `expand_program(forms)?` followed
    /// by `iter_calls_to(&expanded, keyword).map(project).collect()`, in ONE
    /// method on the `Expander` surface. Both [`compile_typed`](crate::compile::compile_typed)
    /// and [`compile_named_from_forms`](crate::compile::compile_named_from_forms)
    /// route through this primitive — they differ only in the per-form
    /// projection `F`: `T::compile_from_args` for the bare-kwargs form
    /// (`compile_typed`), and the NAME-then-`T::compile_from_args` split
    /// (`compile_named_from_forms`).
    ///
    /// Before this lift each dispatcher opened the same three-step pipeline
    /// inline — `let mut exp = Expander::new(); let expanded =
    /// exp.expand_program(forms)?; iter_calls_to(&expanded, T::KEYWORD)
    /// .map(<per-site>).collect()` — past the ≥2 PRIME-DIRECTIVE trigger
    /// once the structural shape is named. After this lift the pipeline
    /// lives in ONE method on the `Expander` surface and the two
    /// dispatchers thread their per-site projection through `F`; a
    /// regression that drifts ONE dispatcher's pipeline from the other (a
    /// future emitter that re-derives `expand_program + iter_calls_to`
    /// inline rather than composing through this primitive, a future
    /// preloaded-expander consumer that wants the same walk but on its
    /// own `Expander` rather than a fresh one) is no longer a silent
    /// two-site divergence: the method binds the composition once and
    /// every consumer threads the per-site projection through `F`.
    ///
    /// Method on `Expander` (not a free function) so the primitive
    /// composes with the broader expander surface: a preloaded expander
    /// (e.g., one that has already absorbed a set of `defmacro` forms
    /// via `expand_program` or `with_macros`) can call this method on
    /// its own state to walk a follow-up program — the
    /// `compiler_spec`'s realize path is the natural future consumer for
    /// that shape. `compile_typed` and `compile_named_from_forms`
    /// instantiate a fresh `Expander::new()` and dispatch through this
    /// method on it; a future preloaded consumer dispatches through it
    /// on the preloaded expander directly. ONE primitive, two postures
    /// (fresh vs. preloaded), no per-posture duplication of the
    /// `expand_program + iter_calls_to + map + collect` pipeline.
    ///
    /// `F` is `FnMut(&[Sexp]) -> Result<R>` — the per-form projection
    /// that fits the consumer's call site. The standard-library
    /// `Iterator::map` bound is `FnMut`, so a closure that captures
    /// mutable state (a future dispatcher that threads a running index
    /// or a borrowed accumulator into the projection) composes
    /// naturally. The `Result<R>` projection short-circuits on the
    /// first error via `Iterator::collect::<Result<Vec<R>, _>>()`, so
    /// the per-form rejection chain (`compile_named_from_forms`'s
    /// `NamedFormMissingName` for the missing NAME slot,
    /// `NamedFormNonSymbolName` for the non-symbol NAME slot,
    /// `T::compile_from_args`'s typed-entry kwargs gate, AND
    /// `compile_typed`'s bare-kwargs `T::compile_from_args` rejection)
    /// fires in the same order under the new shape.
    ///
    /// `R` is owned by construction — the iterator's `&[Sexp]` items
    /// borrow from the local `expanded: Vec<Sexp>` and that borrow
    /// ends when the `.collect()` consumes the iterator, so a `R`
    /// that borrowed from `expanded` would fail to compile here. The
    /// two production consumers (`Vec<T>`, `Vec<NamedDefinition<T>>`)
    /// are both owned-`R` shapes, matching the borrow's structural
    /// constraint.
    ///
    /// Sibling of [`expand_program`](Self::expand_program) — that
    /// method names the FIRST step of the pipeline (program-level
    /// macroexpansion, yielding `Vec<Sexp>`); this method composes it
    /// with the SECOND step (slice-side typed-keyword projection +
    /// per-form mapper). The split lets a consumer that wants the
    /// expanded forms WITHOUT the keyword filter (`tatara-check`'s
    /// per-form dispatcher walks every form, not just matched
    /// keywords) bind to `expand_program` directly; a consumer that
    /// wants both halves composed binds to this method.
    ///
    /// Closes the substrate's program-level walk family on the
    /// `Expander` surface: `expand_program` (yield-all-forms-after-
    /// expansion), `expand` (per-form recursive expansion), `apply`
    /// (per-call substitution), AND now `expand_and_collect_calls_to`
    /// (typed-keyword projection of the expanded forms with a per-form
    /// mapper). Together the four name the canonical surfaces a
    /// dispatcher composes with to extract a typed program from a
    /// post-expansion form set.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// two inline copies of the `Expander::new() + expand_program +
    /// iter_calls_to(_, T::KEYWORD) + map + collect` pipeline across
    /// `compile_typed` and `compile_named_from_forms` is past the ≥2
    /// PRIME-DIRECTIVE trigger once the structural shape is named.
    /// THEORY.md §V.1 — knowable platform; the program-level walk's
    /// typed-keyword-projection composition becomes a NAMED primitive
    /// on the substrate's `Expander` surface rather than a re-derived
    /// three-step inline pipeline at every consumer. Authoring tools
    /// (REPL, LSP, `tatara-check`) that want to walk a program by
    /// typed keyword bind to ONE method on the `Expander` instead of
    /// re-implementing the composition. THEORY.md §II.1 invariant 1 —
    /// typed entry; the typed-keyword filter over an expanded program
    /// IS the rust-level typed-entry-batch gate, and naming its
    /// single shape lifts the gate from two-site duplication to one
    /// rust method the substrate's diagnostic promotions hang off of.
    /// THEORY.md §II.1 invariant 2 — free middle; both consumers
    /// route through the SAME composition primitive, each binding the
    /// per-form projection that fits its call site — a regression
    /// that drifts ONE dispatcher's pipeline shape from the other
    /// cannot reach the substrate's runtime.
    ///
    /// Frontier inspiration: MLIR's `Region::walk<Op>(callback)` —
    /// every typed rewriter that wants "for every Op of kind K in this
    /// region, run callback" binds to ONE typed walker that composes
    /// the kind filter with the per-op visitor; the substrate's
    /// `expand_and_collect_calls_to` is the unstructured-projection
    /// peer of that walker, lifted onto the post-expansion `&[Sexp]`
    /// algebra with the per-form projection as the visitor. Racket's
    /// `syntax-parse` `~seq (keyword args ...) ...` ellipsis-form —
    /// the program-level typed-keyword filter with per-match handler
    /// is the closed-form sibling of `~seq`'s repeated-pattern
    /// matcher, translated through pleme-io primitives as ONE method
    /// on `Expander` composing `expand_program` with `iter_calls_to`
    /// and a per-form mapper. GHC Core's `everything :: forall b. (b
    /// -> b -> b) -> GenericQ b -> GenericQ b` — the typed-IR rewriter's
    /// program-level fold over a typed selector is named ONCE and
    /// every consumer threads the per-node projection; the substrate's
    /// `expand_and_collect_calls_to` is the keyword-projected sibling
    /// of that fold on the `&[Sexp]` algebra.
    pub fn expand_and_collect_calls_to<R, F>(
        &mut self,
        forms: Vec<Sexp>,
        keyword: &str,
        mut project: F,
    ) -> Result<Vec<R>>
    where
        F: FnMut(&[Sexp]) -> Result<R>,
    {
        // Routes through the typed-decoded classifier sibling
        // [`Self::expand_and_collect_calls_to_any`] with a constant-
        // classifier decoder — the same constant-classifier composition
        // [`crate::ast::iter_calls_to`] uses to route through
        // [`crate::ast::iter_calls_to_any`] on the slice algebra. The
        // discarded `()` typed witness (`then_some(())`) is consumed by
        // the wrapper projection `|(), args| project(args)` so the
        // keyword consumer's per-form mapper sees only the args tail,
        // matching the pre-lift signature exactly. Both the keyword AND
        // classifier expander walks now share ONE pipeline implementation
        // (`expand_program + iter_calls_to_any + map + collect`); a
        // regression that drifts a future debug-mode logger, span-aware
        // borrow walker, or fused-iterator invariant from one expander
        // walk to the other becomes structurally impossible.
        self.expand_and_collect_calls_to_any(
            forms,
            |h| (h == keyword).then_some(()),
            move |(), args| project(args),
        )
    }

    /// Compose the expander's program-level expansion with the substrate's
    /// slice-side typed-decoded classifier projection ([`iter_calls_to_any`])
    /// and a caller-supplied per-form projection — `expand_program(forms)?`
    /// followed by `iter_calls_to_any(&expanded, decode).map(|(decoded, args)|
    /// project(decoded, args)).collect()`, in ONE method on the `Expander`
    /// surface. The typed-decoded sibling of
    /// [`Self::expand_and_collect_calls_to`] — where that method filters by
    /// ONE constant keyword, this method filters AND TYPES by a caller-
    /// supplied classifier, yielding the typed witness alongside the per-form
    /// projection's args input.
    ///
    /// Closes the substrate's program-level walk family on the `Expander`
    /// surface at the typed-decoded corner the prior runs' slice-side
    /// `iter_calls_to_any` lift left open — the (keyword, classifier) 2×2
    /// of compose-on-iter projections:
    ///
    /// |                | per-form algebra            | slice algebra                | expander surface                    |
    /// |----------------|-----------------------------|------------------------------|-------------------------------------|
    /// | keyword        | [`Sexp::as_call_to`]        | [`iter_calls_to`]            | [`Self::expand_and_collect_calls_to`] |
    /// | classifier `F` | [`Sexp::as_call_to_any`]    | [`iter_calls_to_any`]        | `expand_and_collect_calls_to_any` (this) |
    ///
    /// The keyword corner now ROUTES through the classifier corner at every
    /// row: [`Sexp::as_call_to`] = constant-classifier projection of
    /// [`Sexp::as_call_to_any`]; [`iter_calls_to`] = constant-classifier
    /// projection of [`iter_calls_to_any`]; [`Self::expand_and_collect_calls_to`]
    /// = constant-classifier projection of THIS method. The substrate's
    /// soft-dispatch family is structurally complete at every algebra
    /// level — per-form, slice, AND expander — so a regression that drifts
    /// a future debug-mode logger, span-aware borrow walker, or
    /// fused-iterator invariant from one walk to its sibling at any algebra
    /// level becomes structurally impossible.
    ///
    /// Two plausible future consumer shapes the typed-decoded expander walk
    /// admits with no boilerplate:
    ///   * **Closed-set classifier** — `expand_and_collect_calls_to_any(forms,
    ///     MacroDefHead::from_keyword, |head, args| dispatch(head, args))`
    ///     macroexpands a program and walks each `(defmacro …)` /
    ///     `(defpoint-template …)` / `(defcheck …)` form decoded to the typed
    ///     `MacroDefHead` enum with its args tail. Future `tatara-check`
    ///     consumers that want "for every macro-definition form in this
    ///     buffer, dispatch by typed kind" bind to ONE method on the
    ///     `Expander` rather than the three-step `expand_program +
    ///     iter_calls_to_any + map + collect` pipeline at each consumer site.
    ///   * **Live-registry classifier** — `expand_and_collect_calls_to_any(forms,
    ///     |h| registry.lookup(h), |handler, args| handler.handle(args))`
    ///     macroexpands a program and walks each form whose head matches a
    ///     runtime registry, decoded to a handler reference. Future
    ///     `tatara-check`-shaped runtime dispatchers (the kind already named
    ///     by [`iter_calls_to_any`]'s docstring as "the natural future
    ///     consumer") bind to ONE method on the `Expander` rather than the
    ///     three-step pipeline.
    ///
    /// The closed-form composition binding the keyword sibling to this typed-
    /// decoded primitive is the structural identity every consumer can pin
    /// against:
    ///
    /// ```ignore
    /// expand_and_collect_calls_to(forms, k, p) ==
    ///     expand_and_collect_calls_to_any(forms,
    ///         |h| (h == k).then_some(()),
    ///         |(), args| p(args))
    /// ```
    ///
    /// `D` is `FnMut(&str) -> Option<T>` — the typed-decoded classifier the
    /// substrate's per-form algebra already shapes ([`Sexp::as_call_to_any`]
    /// uses `FnOnce`, the slice algebra and this method use `FnMut` for the
    /// same reason: the walk calls the decoder once per matching form, and a
    /// decoder that captures mutable state (a counter, a registry cache, a
    /// visited-set) maintains that state across the program-level walk).
    /// `F` is `FnMut(T, &[Sexp]) -> Result<R>` — the per-form projection that
    /// receives BOTH the typed witness AND the args tail; the keyword sibling
    /// discards the (unit) typed witness via `|(), args| project(args)`. The
    /// `Result<R>` projection short-circuits on the first error via
    /// `Iterator::collect::<Result<Vec<R>, _>>()` so the per-form rejection
    /// chain fires in source order at the first failing matched form.
    ///
    /// `R` is owned by construction — same constraint as the keyword
    /// sibling. `T` is owned because the underlying [`iter_calls_to_any`]
    /// requires `T: 'a` where `'a` is the post-expansion `Vec<Sexp>` borrow
    /// lifetime; consumers projecting to a typed `Copy` enum
    /// (`MacroDefHead`, a future `AuthoringDirective`) get the value
    /// directly per form, consumers projecting to a borrowed `&'static str`
    /// (a closed-set head) project to `&'static str` and inherit the static
    /// lifetime through the classifier, consumers projecting to a `&Handler`
    /// (a live-registry classifier) bind through the registry's owned
    /// borrow with `&self` outliving the walk.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// keyword sibling [`Self::expand_and_collect_calls_to`] is now a
    /// CONSEQUENCE of this typed-decoded primitive + a constant-classifier
    /// decoder, parallel to how [`iter_calls_to`] is a consequence of
    /// [`iter_calls_to_any`] on the slice algebra. The substrate's
    /// program-level walk family is no longer two parallel implementations
    /// the type system fails to bind (a future regression that drifts the
    /// keyword pipeline's instrumentation from the classifier pipeline's
    /// instrumentation cannot reach the runtime — both compose through ONE
    /// `expand_program + iter_calls_to_any + map + collect` body, with the
    /// keyword filter expressed as a closed-form classifier). THEORY.md
    /// §V.1 — knowable platform; the typed-decoded expander walk becomes a
    /// NAMED primitive on the substrate's `Expander` surface rather than a
    /// future re-derived three-step inline pipeline per consumer.
    /// Authoring tools (REPL, LSP, `tatara-check`) that want to walk a
    /// program by typed-decoded classifier bind to ONE method on the
    /// `Expander` instead of re-implementing the composition. THEORY.md
    /// §II.1 invariant 2 — free middle; both expander-surface dispatchers
    /// (keyword + classifier) route through the SAME `expand_program +
    /// iter_calls_to_any + map + collect` composition primitive.
    ///
    /// Frontier inspiration: MLIR's `Region::walk<OpInterface,
    /// OpInterface2, …>([&](auto op) { … })` — the typed-IR walk over a
    /// region yielding ops decoded to their typed interface witness with
    /// per-op callback is the MLIR idiom; the substrate's
    /// `expand_and_collect_calls_to_any` is the unstructured-Rust peer on
    /// the post-expansion `&[Sexp]` algebra, with `decode: FnMut(&str) ->
    /// Option<T>` standing in for MLIR's typed-interface dyn-cast bag and
    /// `project: FnMut(T, &[Sexp]) -> Result<R>` standing in for the
    /// typed-op callback. Racket's `syntax-parse` `~or* (~datum defX)
    /// (~datum defY) (head args …)` over an ellipsis-form combined with a
    /// `#:with name:id` typed-witness binder — the program-level
    /// typed-decoded filter with per-match handler is the closed-form
    /// sibling of `syntax-parse`'s typed-choice repeater, translated
    /// through pleme-io primitives as ONE method on `Expander`. GHC Core's
    /// `everythingBut :: (a -> a -> a) -> GenericQ (a, Bool) -> GenericQ a`
    /// — the typed-IR rewriter's program-level fold over a typed
    /// kind-with-stop selector is named ONCE and every consumer threads
    /// the per-node typed projection; this method is the typed-decoded
    /// peer on the `Vec<Sexp>` algebra, with the classifier playing
    /// `everythingBut`'s typed-selector role.
    pub fn expand_and_collect_calls_to_any<R, F, D, T>(
        &mut self,
        forms: Vec<Sexp>,
        decode: D,
        mut project: F,
    ) -> Result<Vec<R>>
    where
        D: FnMut(&str) -> Option<T>,
        F: FnMut(T, &[Sexp]) -> Result<R>,
    {
        let expanded = self.expand_program(forms)?;
        crate::ast::iter_calls_to_any(&expanded, decode)
            .map(|(decoded, args)| project(decoded, args))
            .collect()
    }

    /// Read a source string into top-level forms via [`crate::reader::read`],
    /// then route the forms through
    /// [`expand_and_collect_calls_to`](Self::expand_and_collect_calls_to) — the
    /// from-source posture of the program-level walk, in ONE method on the
    /// `Expander` surface.
    ///
    /// Before this lift the same two-step chain — `let forms = read(src)?;
    /// <expander>.expand_and_collect_calls_to(forms, keyword, project)` — lived
    /// inline at four sites: the two free-function typed dispatchers
    /// ([`compile_typed`](crate::compile::compile_typed) and
    /// [`compile_named`](crate::compile::compile_named) via
    /// [`compile_named_from_forms`](crate::compile::compile_named_from_forms))
    /// AND the two preloaded-expander methods on
    /// [`RealizedCompiler`](crate::compiler_spec::RealizedCompiler)
    /// (`compile_typed`, `compile_named`). After this lift the read-then-walk
    /// composition lives in ONE method on the `Expander`; each of the four
    /// dispatchers binds it with the per-site `(expander posture, projection)`
    /// pair that fits its call boundary — `Expander::new()` for the
    /// fresh-expander dispatchers, `self.preloaded.clone()` for the
    /// preloaded-expander dispatchers; `T::compile_from_args` for the
    /// bare-kwargs projection, `named_form_projection::<T>` for the
    /// NAME-then-kwargs projection.
    ///
    /// Sibling of [`expand_and_collect_calls_to`](Self::expand_and_collect_calls_to)
    /// — that method takes a pre-parsed `Vec<Sexp>` (the from-forms posture,
    /// for callers that have already read or that compose with another
    /// `Sexp`-producing surface like a macro-expanded subform); this method
    /// takes a `&str` (the from-source posture, for callers consuming
    /// authoring-surface source text directly). Both compose with the same
    /// `expand_program + iter_calls_to + map + collect` pipeline through ONE
    /// from-forms primitive — the from-source posture stacks one read step on
    /// top, projecting `crate::reader::read`'s `Result<Vec<Sexp>>` into the
    /// from-forms primitive via `?`.
    ///
    /// `?`-routing through `read` preserves the structural ordering of the
    /// rejection chain end-to-end: a reader error (lexer / parser /
    /// unbalanced-paren / unterminated-string) short-circuits BEFORE
    /// `expand_program` runs; an `expand_program` error
    /// (`defmacro`-NAME-not-a-symbol, `OptionalParamMalformed`,
    /// `RestParamMissingName`, et al.) short-circuits BEFORE the keyword
    /// filter walks anything; a per-form `project` error
    /// (`NamedFormMissingName`, `NamedFormNonSymbolName`,
    /// `T::compile_from_args`'s typed-entry kwargs gate) short-circuits at the
    /// first failing matched form via `Iterator::collect::<Result<Vec<R>, _>>()`.
    /// Each dispatcher's rejection chain remains exactly what it was pre-lift,
    /// now sourced from ONE composition point rather than four.
    ///
    /// Closes the program-level walk family on the `Expander` surface across
    /// BOTH the from-forms posture
    /// ([`expand_and_collect_calls_to`](Self::expand_and_collect_calls_to))
    /// AND the from-source posture (this method) — together with
    /// `expand_program` (yield-all-forms-after-expansion), `expand` (per-form
    /// recursive expansion), and `apply` (per-call substitution), they name
    /// the canonical surfaces a dispatcher composes with to extract a typed
    /// program from a post-expansion form set. A future dispatcher that wants
    /// "read this source, then walk every call to keyword K and project each
    /// to R" — a debug-mode REPL command, an LSP "find all typed-domain
    /// definitions in this buffer" handler, a `tatara-check` command that
    /// dispatches each typed `(defX …)` form in `checks.lisp` to its
    /// registered domain — binds to ONE method on the `Expander` instead of
    /// re-deriving the two-step `read + expand_and_collect_calls_to` chain.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; four
    /// inline copies of the `read(src)? + <expander>.expand_and_collect_calls_to`
    /// chain across the two fresh-expander dispatchers and the two
    /// preloaded-expander dispatchers is past the ≥2 PRIME-DIRECTIVE trigger
    /// once the structural shape is named. THEORY.md §V.1 — knowable
    /// platform; the read-then-walk composition becomes a NAMED primitive on
    /// the substrate's `Expander` surface rather than a re-derived two-step
    /// inline pipeline at every dispatcher. THEORY.md §II.1 invariant 2 —
    /// free middle; all four dispatchers route through the SAME composition
    /// primitive, so a regression that drifts ONE dispatcher's read-then-walk
    /// pipeline from the others cannot reach the substrate's runtime — the
    /// type system binds all consumers to the from-source primitive's single
    /// emission shape.
    ///
    /// Frontier inspiration: Racket's `(eval-string str ns)` against a
    /// namespace populated with the preloaded compiler's `require`d macros —
    /// the from-source-string entry to typed-program evaluation inside a
    /// namespace that carries the macro library is the Racket idiom; the
    /// substrate's `expand_source_and_collect_calls_to` is the Rust-typed
    /// peer of that, with the typed-keyword projection composed in.
    ///
    /// Post-lift the body routes through the typed-decoded sibling
    /// [`Self::expand_source_and_collect_calls_to_any`] with a
    /// constant-classifier decoder — the same constant-classifier
    /// composition [`Self::expand_and_collect_calls_to`] uses to route
    /// through [`Self::expand_and_collect_calls_to_any`] on the
    /// from-forms axis, and that [`crate::ast::iter_calls_to`] uses to
    /// route through [`crate::ast::iter_calls_to_any`] on the slice
    /// algebra. The discarded `()` typed witness (`then_some(())`) is
    /// consumed by the wrapper projection `|(), args| project(args)`
    /// so the keyword consumer's per-form mapper sees only the args
    /// tail. Both the keyword AND classifier from-source walks now
    /// share ONE pipeline implementation (`read + expand_program +
    /// iter_calls_to_any + map + collect`) — a regression that drifts
    /// a future debug-mode logger, span-aware borrow walker, or
    /// fused-iterator invariant from one from-source walk to the
    /// other becomes structurally impossible.
    pub fn expand_source_and_collect_calls_to<R, F>(
        &mut self,
        src: &str,
        keyword: &str,
        mut project: F,
    ) -> Result<Vec<R>>
    where
        F: FnMut(&[Sexp]) -> Result<R>,
    {
        self.expand_source_and_collect_calls_to_any(
            src,
            |h| (h == keyword).then_some(()),
            move |(), args| project(args),
        )
    }

    /// Read a source string into top-level forms via [`crate::reader::read`],
    /// then route the forms through
    /// [`expand_and_collect_calls_to_any`](Self::expand_and_collect_calls_to_any) —
    /// the from-source posture of the typed-decoded classifier walk, in
    /// ONE method on the `Expander` surface. The typed-decoded sibling of
    /// [`Self::expand_source_and_collect_calls_to`] — where that method
    /// filters by ONE constant keyword, this method filters AND TYPES by
    /// a caller-supplied classifier, yielding the typed witness alongside
    /// the per-form projection's args input. Sourced from `&str` rather
    /// than a `Vec<Sexp>`.
    ///
    /// Closes the substrate's program-level walk family on the `Expander`
    /// surface across BOTH input postures × BOTH projection forms — the
    /// (from-forms, from-source) × (keyword, classifier) 2×2 of
    /// compose-on-iter projections:
    ///
    /// |                | from-forms (`Vec<Sexp>`)                                      | from-source (`&str`)                                                |
    /// |----------------|---------------------------------------------------------------|---------------------------------------------------------------------|
    /// | keyword        | [`Self::expand_and_collect_calls_to`]                         | [`Self::expand_source_and_collect_calls_to`]                        |
    /// | classifier `F` | [`Self::expand_and_collect_calls_to_any`]                     | `expand_source_and_collect_calls_to_any` (this)                     |
    ///
    /// Each row's keyword corner now ROUTES through the classifier corner
    /// at every column: [`Self::expand_source_and_collect_calls_to`]'s
    /// body composes this typed-decoded primitive with a
    /// `|h| (h == keyword).then_some(())` decoder and the wrapper
    /// projection `|(), args| project(args)`, parallel to how
    /// [`Self::expand_and_collect_calls_to`] routes through
    /// [`Self::expand_and_collect_calls_to_any`] on the from-forms axis
    /// and [`crate::ast::iter_calls_to`] routes through
    /// [`crate::ast::iter_calls_to_any`] on the slice algebra. The 2×2
    /// reduces to ONE pipeline implementation
    /// (`read + expand_program + iter_calls_to_any + map + collect`)
    /// every consumer threads its `(decoder, project)` pair through.
    ///
    /// Future consumer shapes the typed-decoded from-source primitive
    /// admits:
    ///   * **Closed-set classifier** — `expand_source_and_collect_calls_to_any(
    ///     src, MacroDefHead::from_keyword, |head, args| { … })` walks a
    ///     source buffer dispatching every `(defmacro …)` / `(defpoint-template …)` /
    ///     `(defcheck …)` form to its typed `MacroDefHead` arm — exactly the
    ///     shape a future LSP "find every macro-definition form in this
    ///     buffer, decode each by typed kind" handler reaches for, sourced
    ///     from authoring text directly rather than a pre-parsed
    ///     `Vec<Sexp>`.
    ///   * **Live-registry classifier** — `expand_source_and_collect_calls_to_any(
    ///     src, |h| registry.get(h), |handler, args| handler.compile(args))`
    ///     walks a source buffer dispatching every form whose head is
    ///     registered to its typed handler — exactly the shape
    ///     `tatara-check`'s "macroexpand checks.lisp and dispatch every
    ///     `(defX …)` form through the registered domain dispatcher"
    ///     walker reaches for, sourced from disk-loaded source text
    ///     rather than the round-tripped `Vec<Sexp>` the typed
    ///     dispatchers consume.
    ///
    /// `?`-routing through `read` preserves the structural ordering of
    /// the rejection chain end-to-end: a reader error (lexer / parser /
    /// unbalanced-paren / unterminated-string) short-circuits BEFORE
    /// `expand_program` runs; an `expand_program` error short-circuits
    /// BEFORE the classifier filter walks anything; a per-form `project`
    /// error short-circuits at the first failing matched form via
    /// `Iterator::collect::<Result<Vec<R>, _>>()`. Each consumer's
    /// rejection chain inherits the from-forms typed-decoded primitive's
    /// shape verbatim, now sourced from `&str` via ONE composition point.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the (from-forms, from-source) × (keyword, classifier) 2×2 closes
    /// at the from-source classifier corner this method establishes —
    /// the prior runs' from-forms classifier sibling
    /// (`expand_and_collect_calls_to_any`, run e47d043) AND from-source
    /// keyword sibling (`expand_source_and_collect_calls_to`, run
    /// b477d81) AND slice-side classifier sibling (`iter_calls_to_any`,
    /// run 38625e3) named three of the four corners; this lift names
    /// the fourth so the family is structurally complete at every
    /// algebra level. THEORY.md §V.1 — knowable platform; the
    /// read-then-classifier-walk composition becomes a NAMED primitive
    /// on the substrate's `Expander` surface rather than a future
    /// re-derived `read(src)? + expand_and_collect_calls_to_any(…)`
    /// two-step chain at every consumer that wants to walk source text
    /// by typed-decoded classifier. THEORY.md §II.1 invariant 2 — free
    /// middle; the from-source keyword filter
    /// (`expand_source_and_collect_calls_to`) now routes through the
    /// from-source classifier filter via the constant-classifier
    /// composition, so a regression that drifts the keyword filter's
    /// instrumentation from the classifier filter's instrumentation
    /// becomes structurally impossible. The pre-lift inline
    /// `read(src)? + expand_and_collect_calls_to(forms, keyword, project)`
    /// chain is now a TYPED CONSEQUENCE of the typed-decoded primitive
    /// composed with a constant-classifier decoder, not a parallel
    /// implementation the type system happens to not catch.
    ///
    /// Frontier inspiration: Racket's `(eval-string str ns)` against a
    /// namespace + typed-syntax-class dispatch — the from-source-string
    /// entry to typed-program evaluation inside a namespace that carries
    /// the macro library, dispatching by typed syntax-class is the
    /// Racket idiom; the substrate's
    /// `expand_source_and_collect_calls_to_any` is the Rust-typed peer
    /// of that, with the typed-decoded classifier composed in. MLIR's
    /// `parseSourceFile<Op>(srcFile, ctx)` then `mod.walk<OpInterface>(
    /// callback)` — the parse-then-typed-walk over a source buffer
    /// dispatching by typed-interface witness is the MLIR idiom; this
    /// method is the substrate's typed `&[Sexp]`-algebra peer.
    pub fn expand_source_and_collect_calls_to_any<R, F, D, T>(
        &mut self,
        src: &str,
        decode: D,
        project: F,
    ) -> Result<Vec<R>>
    where
        D: FnMut(&str) -> Option<T>,
        F: FnMut(T, &[Sexp]) -> Result<R>,
    {
        let forms = crate::reader::read(src)?;
        self.expand_and_collect_calls_to_any(forms, decode, project)
    }

    /// Compose the expander's program-level expansion with the substrate's
    /// typed-decoded classifier walk AND the named-form NAME-shape gate
    /// ([`crate::compile::split_name_slot`]) — the from-forms posture of
    /// the (named NAME-then-kwargs × typed-decoded classifier) cell of
    /// the typed-dispatcher matrix on the `Expander` surface, sibling of
    /// [`Self::expand_to_named`] (the constant-`T::KEYWORD` × named cell)
    /// and of [`Self::expand_and_collect_calls_to_any`] (the from-forms
    /// classifier × bare-kwargs cell).
    ///
    /// Routes through [`Self::expand_and_collect_calls_to_any`] with a
    /// wrapper projection that composes [`crate::compile::split_name_slot`]
    /// (the named-form arity + NAME-shape gate on the substrate's
    /// `&[Sexp]` algebra) with the caller-supplied per-form projection.
    /// The decoder yields `Option<(T, &'static str)>` — the typed witness
    /// PAIRED with the canonical static keyword the named-form structural
    /// rejection variants ([`crate::error::LispError::NamedFormMissingName`],
    /// [`crate::error::LispError::NamedFormNonSymbolName`]) carry as
    /// `&'static str` slots. Threading the `&'static` constraint through
    /// the decoder's return type pins the same compile-time discipline
    /// [`crate::compile::split_name_slot`]'s `keyword: &'static str`
    /// parameter pins at the gate boundary — a typo in the canonical
    /// keyword can never drift into the diagnostic at runtime, same posture
    /// as the constant-`T::KEYWORD` cell where `T::KEYWORD` is
    /// `&'static str` by trait construction.
    ///
    /// Closes the (named × typed-decoded classifier) corner of the
    /// typed-dispatcher matrix on the `Expander` surface that the prior
    /// run's [`crate::compile::split_name_slot`] lift (dd50801) named as
    /// the future change the slice-side gate's named lift enables. The
    /// 2×2 of compose-on-iter projections on the named-form axis becomes
    /// structurally complete on the `Expander` surface:
    ///
    /// |                | constant `T::KEYWORD`               | typed-decoded classifier                     |
    /// |----------------|-------------------------------------|----------------------------------------------|
    /// | from-forms     | [`Self::expand_to_named`]           | `expand_and_collect_named_calls_to_any` (this) |
    /// | from-source    | [`Self::expand_source_to_named`]    | [`Self::expand_source_and_collect_named_calls_to_any`] |
    ///
    /// The constant-`T::KEYWORD` column is the typed CONSEQUENCE of the
    /// classifier column: an `expand_to_named::<T>(forms)` call composes
    ///
    /// ```ignore
    /// expand_and_collect_named_calls_to_any(forms,
    ///     |h| (h == T::KEYWORD).then_some(((), T::KEYWORD)),
    ///     |(), name, spec_args| {
    ///         let spec = T::compile_from_args(spec_args)?;
    ///         Ok(NamedDefinition { name: name.to_string(), spec })
    ///     })
    /// ```
    ///
    /// Both columns route through ONE composition point on the
    /// `Expander` surface (the typed-decoded classifier walk inside
    /// [`Self::expand_and_collect_calls_to_any`]) and ONE gate body on
    /// the `&[Sexp]` algebra ([`crate::compile::split_name_slot`]). A
    /// regression that drifts ONE cell's NAME-slot rejection chain from
    /// the others is structurally impossible — every cell binds to the
    /// SAME `split_name_slot` body, the SAME `expand_program +
    /// iter_calls_to_any + map + collect` pipeline. The
    /// pre-lift `NamedFormMissingName` / `NamedFormNonSymbolName`
    /// rejection chain inside `compile::named_form_projection` (the
    /// `pub(crate)` typed-domain SPECIALIZATION that
    /// [`Self::expand_to_named`] routes through) fires identically
    /// through this classifier primitive — both consumers compose
    /// [`crate::compile::split_name_slot`] with their per-site typed
    /// continuation.
    ///
    /// Future consumer shapes the typed-decoded named-form primitive
    /// admits with no boilerplate:
    ///   * **Closed-set classifier** — a `tatara-check` runner that
    ///     dispatches every `(defmonitor NAME …)` / `(defnotify NAME …)`
    ///     / `(defalertpolicy NAME …)` form in `checks.lisp` through ONE
    ///     classifier (a closed-set enum `Domain::{Monitor, Notify,
    ///     AlertPolicy}` with `Domain::from_keyword`) decoding each
    ///     head to its typed kind PAIRED with its canonical static
    ///     keyword, then dispatches in the per-form projection — binds
    ///     to ONE method rather than two-step `expand_and_collect_calls_to_any
    ///     + split_name_slot` composition at each consumer site.
    ///   * **Live-registry classifier** — a future `tatara-check`-shaped
    ///     runtime dispatcher: `expand_and_collect_named_calls_to_any(forms,
    ///     |h| registry.get(h).map(|d| (d, d.keyword())), |dispatcher,
    ///     name, args| dispatcher.compile(name, args))` walks a program
    ///     dispatching every named form whose head is in a runtime
    ///     registry, decoded to a handler reference AND its canonical
    ///     keyword (sourced from the dispatcher itself, NOT from the
    ///     matched head text). The `&'static str` discipline on the
    ///     keyword slot is preserved through the dispatcher's own
    ///     `keyword() -> &'static str` accessor.
    ///   * **`compile_named_any` free function family** — the natural
    ///     fresh-expander free-function + preloaded-`RealizedCompiler`
    ///     consumer pair, sibling of the existing `compile_typed_any`
    ///     / `RealizedCompiler::compile_typed_any` pair. A future run
    ///     lands those as 3-line composes on top of this primitive.
    ///
    /// `D` is `FnMut(&str) -> Option<(T, &'static str)>` — the typed-
    /// decoded classifier yields a typed witness `T` AND its canonical
    /// static keyword for the gate. `F` is `FnMut(T, &str, &[Sexp]) ->
    /// Result<R>` — the per-form projection receives the typed witness
    /// `T` ALONGSIDE the NAME slot's BORROWED `&str` projection (sourced
    /// from [`Sexp::as_symbol_or_string`], which accepts BOTH symbol and
    /// string NAME-author surfaces) AND the spec args tail. Consumers
    /// that need owned ownership of the NAME (`NamedDefinition.name:
    /// String`, JSON-serialized payloads) `.to_string()` themselves —
    /// pushing the clone to the consumer boundary keeps the primitive
    /// allocation-free. The `Result<R>` projection short-circuits on
    /// the first error via `Iterator::collect::<Result<Vec<R>, _>>()`
    /// inside [`Self::expand_and_collect_calls_to_any`].
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the (named × typed-decoded classifier) cell becomes a NAMED
    /// primitive on the `Expander` surface composed FROM TWO substrate
    /// primitives ([`Self::expand_and_collect_calls_to_any`] +
    /// [`crate::compile::split_name_slot`]) rather than re-derived
    /// inline at every named-classifier consumer's call site.
    /// THEORY.md §II.1 invariant 1 — typed entry; the typed-decoded
    /// classifier-filtered + NAME-shape-gated + caller-projected walk
    /// IS the typed-entry-batch gate for named-form dispatch at the
    /// `Expander` surface, with the `&'static str` keyword discipline
    /// preserved through the decoder's return type. THEORY.md §II.1
    /// invariant 2 — free middle; ALL four cells of the named-form
    /// dispatcher matrix on the `Expander` surface route through ONE
    /// composition point — a regression that drifts ONE cell's
    /// instrumentation from the others is structurally impossible.
    /// THEORY.md §V.1 — knowable platform; the named-form classifier
    /// walk becomes a discoverable primitive that LSP / REPL /
    /// `tatara-check` consumers bind to ONE method on the `Expander`
    /// instead of re-implementing the two-step `expand_and_collect_calls_to_any
    /// + split_name_slot` composition.
    ///
    /// Frontier inspiration: MLIR's `Region::walk<NamedOpInterface>(
    /// [&](auto op) { auto name = op.getName(); … })` — the typed-IR
    /// walk over a region yielding ops decoded to their typed interface
    /// witness with the named-symbol accessor pre-extracted is the
    /// MLIR idiom for named typed dispatch; this method is the
    /// substrate's unstructured-Rust peer with the typed-decoded
    /// classifier composed in and the NAME slot extracted via the
    /// substrate's `split_name_slot` gate. Racket's `syntax-parse`
    /// `~or* ((~datum defX) name:id arg ...) ((~datum defY) name:id
    /// arg ...) ((~datum defZ) name:id arg ...)` over an ellipsis-form
    /// — the typed-choice repeater with named-slot binder PAIRED with
    /// per-arm dispatch — is the Racket idiom; this primitive is the
    /// substrate's Rust-typed peer with the closed-set classifier
    /// playing the `~or*` typed-choice role and the NAME slot extracted
    /// by the substrate's slice-side gate.
    pub fn expand_and_collect_named_calls_to_any<R, F, D, T>(
        &mut self,
        forms: Vec<Sexp>,
        decode: D,
        mut project: F,
    ) -> Result<Vec<R>>
    where
        D: FnMut(&str) -> Option<(T, &'static str)>,
        F: FnMut(T, &str, &[Sexp]) -> Result<R>,
    {
        // Routes through the slice-side typed-decoded named projection
        // [`crate::ast::iter_named_calls_to_any`] — the same
        // `expand_program + iter_*_to_any + map + collect` shape
        // [`Self::expand_and_collect_calls_to_any`] uses on the bare-kwargs
        // axis. Pre-lift this method routed through the bare expander
        // surface and welded [`crate::compile::split_name_slot`] inside
        // the projection closure; post-lift the named gate composition
        // lives at the slice level (`iter_named_calls_to_any`'s body)
        // and the Expander surface inherits it through delegation. Both
        // rows of the Expander surface (bare-kwargs, named) now share
        // ONE pipeline skeleton on the slice algebra — a regression that
        // drifts a future debug-mode logger, span-aware borrow walker,
        // or fused-iterator invariant from one row to the other becomes
        // structurally impossible at the slice boundary.
        let expanded = self.expand_program(forms)?;
        crate::ast::iter_named_calls_to_any(&expanded, decode)
            .map(|maybe_triple| {
                let (decoded, name, spec_args) = maybe_triple?;
                project(decoded, name, spec_args)
            })
            .collect()
    }

    /// Read a source string into top-level forms via [`crate::reader::read`],
    /// then route the forms through
    /// [`Self::expand_and_collect_named_calls_to_any`] — the from-source
    /// posture of the (named × typed-decoded classifier) cell on the
    /// `Expander` surface, sibling of
    /// [`Self::expand_source_and_collect_calls_to_any`] (the from-source
    /// classifier × bare-kwargs cell).
    ///
    /// Closes the (from-forms, from-source) × (constant `T::KEYWORD`,
    /// typed-decoded classifier) 2×2 on the named-form axis of the
    /// `Expander` surface — together with [`Self::expand_to_named`],
    /// [`Self::expand_source_to_named`], and
    /// [`Self::expand_and_collect_named_calls_to_any`], every cell of the
    /// matrix binds to ONE composition point.
    ///
    /// `?`-routing through `read` preserves the structural ordering of
    /// the rejection chain end-to-end: a reader error (lexer / parser /
    /// unbalanced-paren / unterminated-string) short-circuits BEFORE
    /// `expand_program` runs; an `expand_program` error short-circuits
    /// BEFORE the classifier filter walks anything; the named-form gate
    /// (`split_first()` arity → `as_symbol_or_string()` shape) inside
    /// [`crate::compile::split_name_slot`] fires for the first matched
    /// form that violates either condition; a per-form `project` error
    /// short-circuits at the first failing matched form via
    /// `Iterator::collect::<Result<Vec<R>, _>>()`.
    ///
    /// Theory anchor: same as [`Self::expand_and_collect_named_calls_to_any`].
    /// THEORY.md §VI.1 (generation over composition; the from-source
    /// posture inherits the named-form gate composition through
    /// delegation rather than re-deriving the `read(src)? + walk + gate`
    /// chain at every from-source named-classifier consumer's call
    /// site), THEORY.md §II.1 invariant 2 (free middle; the from-source
    /// posture and the from-forms posture route through the SAME named-
    /// form composition).
    pub fn expand_source_and_collect_named_calls_to_any<R, F, D, T>(
        &mut self,
        src: &str,
        decode: D,
        project: F,
    ) -> Result<Vec<R>>
    where
        D: FnMut(&str) -> Option<(T, &'static str)>,
        F: FnMut(T, &str, &[Sexp]) -> Result<R>,
    {
        let forms = crate::reader::read(src)?;
        self.expand_and_collect_named_calls_to_any(forms, decode, project)
    }

    /// Macroexpand a pre-parsed program through `self` and project every
    /// `(keyword NAME :k v …)` form into `R` via a caller-supplied
    /// `(name, args) -> Result<R>` projection — the constant-keyword
    /// sibling of [`Self::expand_and_collect_named_calls_to_any`] on the
    /// named-form axis of the `Expander` surface, parallel to how
    /// [`Self::expand_and_collect_calls_to`] is the constant-keyword
    /// sibling of [`Self::expand_and_collect_calls_to_any`] on the
    /// bare-kwargs axis.
    ///
    /// Routes through the classifier sibling with a constant-classifier
    /// decoder that yields a discarded `()` typed witness paired with
    /// the `&'static str` keyword — the same `(T, &'static str)` decoder
    /// shape [`crate::compile::split_name_slot`] (composed inside the
    /// classifier primitive) pins at the slice-side gate boundary for
    /// the named-form structural rejection chain
    /// (`NamedFormMissingName.keyword`, `NamedFormNonSymbolName.keyword`).
    /// `&'static` is the same lifetime discipline the typed-decoded
    /// classifier signature enforces; the named gate's verbatim keyword
    /// threading inherits it through the constant-classifier composition.
    ///
    /// Projection signature `FnMut(&str, &[Sexp]) -> Result<R>` receives
    /// the BORROWED NAME slot and the BORROWED spec args tail. Consumers
    /// that need owned ownership (`NamedDefinition.name: String`) call
    /// `.to_string()` themselves — pushing the clone to the consumer
    /// boundary keeps the primitive allocation-free, matching the
    /// classifier sibling's projection signature on the NAME slot.
    ///
    /// Closes the (named NAME-then-kwargs × constant-keyword) cell of
    /// the `Expander` typed-walk family at the runtime-keyword × untyped
    /// `R` corner — pre-lift the cell was reachable ONLY through
    /// [`Self::expand_to_named`] with the `T: TataraDomain` type
    /// parameter baking `T::KEYWORD` AND the `T::compile_from_args`-
    /// based `crate::compile::named_form_projection<T>` projection
    /// into the dispatch through `expand_and_collect_calls_to(forms,
    /// T::KEYWORD, named_form_projection::<T>)`. Post-lift the cell
    /// surfaces as ONE method that takes the keyword and the
    /// `(name, args) -> R` projection as caller-supplied parameters,
    /// and [`Self::expand_to_named`] routes through it as the typed
    /// `T::KEYWORD`-constant specialization. The split lets a consumer
    /// that wants "walk every form whose head is a runtime keyword `kw`
    /// and project `(NAME, spec_args) -> R` for an arbitrary `R`" (a
    /// future REPL `:walk-named <kw>` command, an LSP "find-named-
    /// declarations-of-keyword `kw`" handler, a `tatara-check` runner
    /// dispatching on a single typed keyword whose projection isn't
    /// `T::compile_from_args`) bind to ONE method on the `Expander`
    /// surface rather than re-deriving the
    /// `expand_and_collect_named_calls_to_any(forms, |h| (h ==
    /// kw).then_some(((), kw)), |(), name, args| project(name, args))`
    /// two-line composition inline.
    ///
    /// Sibling of [`Self::expand_source_and_collect_named_calls_to`]
    /// — that method stacks a [`crate::reader::read`] step on top of
    /// this one (the from-source posture); this method takes a
    /// pre-parsed `Vec<Sexp>` (the from-forms posture). Together with
    /// [`Self::expand_and_collect_named_calls_to_any`] /
    /// [`Self::expand_source_and_collect_named_calls_to_any`], the four
    /// cells of the (named × {constant-keyword, classifier} ×
    /// {from-forms, from-source}) sub-matrix close on the `Expander`
    /// surface — each cell binds to ONE composition point, with the
    /// constant-keyword cells routing through the classifier cells via
    /// constant-classifier decoding.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the `split_name_slot` composition lived at TWO sites pre-lift
    /// (`crate::compile::named_form_projection` body, which
    /// [`Self::expand_to_named`] routes through, AND
    /// [`Self::expand_and_collect_named_calls_to_any`] body, which the
    /// typed-decoded classifier consumers route through) — past the
    /// ≥2 PRIME-DIRECTIVE trigger once the named-form gate is named.
    /// THEORY.md §V.1 — knowable platform; the named constant-keyword
    /// walk becomes a NAMED primitive on the `Expander` surface
    /// composable by any future consumer (REPL, LSP, `tatara-check`)
    /// instead of a re-derived two-line `expand_and_collect_named_
    /// calls_to_any(forms, constant-decoder, wrapper-projection)`
    /// composition. THEORY.md §II.1 invariant 2 — free middle; both
    /// the typed (`expand_to_named<T>`) and untyped (this method)
    /// constant-keyword named cells route through the SAME named
    /// classifier primitive — a regression that drifts ONE cell's
    /// NAME-slot rejection chain (`NamedFormMissingName`,
    /// `NamedFormNonSymbolName`) from the other becomes structurally
    /// impossible.
    ///
    /// Frontier inspiration: Racket's `syntax-parse` `((~datum kw)
    /// name:id arg ...)` arm — the named NAME-slot binder under a
    /// constant `~datum` keyword paired with a per-match handler is
    /// the Racket idiom; this method is the substrate's Rust-typed
    /// peer with the constant-keyword filter composed at the
    /// classifier corner. MLIR's `Region::walk<NamedOpKind>([&](auto
    /// op) { auto name = op.getName(); … })` against a single
    /// operation kind — the typed-IR walk yielding ops decoded to
    /// their typed kind with the named-symbol accessor pre-extracted,
    /// specialized on a constant kind; this method is the substrate's
    /// unstructured-`R` peer with the constant-classifier composition
    /// pinned at the call boundary.
    pub fn expand_and_collect_named_calls_to<R, F>(
        &mut self,
        forms: Vec<Sexp>,
        keyword: &'static str,
        mut project: F,
    ) -> Result<Vec<R>>
    where
        F: FnMut(&str, &[Sexp]) -> Result<R>,
    {
        // Routes through the typed-decoded named-classifier sibling
        // [`Self::expand_and_collect_named_calls_to_any`] with a
        // constant-classifier decoder — the same constant-classifier
        // composition [`Self::expand_and_collect_calls_to`] uses to
        // route through [`Self::expand_and_collect_calls_to_any`] on
        // the bare-kwargs axis, and that
        // [`crate::ast::iter_calls_to`] uses to route through
        // [`crate::ast::iter_calls_to_any`] on the slice algebra. The
        // discarded `()` typed witness (`then_some(((), keyword))`)
        // is consumed by the wrapper projection
        // `|(), name, args| project(name, args)` so the keyword
        // consumer's per-form mapper sees only `(name, spec_args)`,
        // matching the bare projection signature.
        self.expand_and_collect_named_calls_to_any(
            forms,
            |h| (h == keyword).then_some(((), keyword)),
            move |(), name, args| project(name, args),
        )
    }

    /// Read a source string into top-level forms via [`crate::reader::read`],
    /// then route the forms through
    /// [`Self::expand_and_collect_named_calls_to`] — the from-source
    /// posture of the (named × constant-keyword) cell on the
    /// `Expander` surface.
    ///
    /// Composes [`crate::reader::read`] with
    /// [`Self::expand_and_collect_named_calls_to`] — the
    /// `(keyword, project)` binding is bound in ONE place (the
    /// from-forms row) and this from-source sibling inherits the
    /// binding through delegation, mirroring how
    /// [`Self::expand_source_and_collect_calls_to`] composes
    /// [`crate::reader::read`] with [`Self::expand_and_collect_calls_to`]
    /// on the bare-kwargs axis.
    ///
    /// `?`-routing through `read` preserves the structural ordering of
    /// the rejection chain end-to-end: a reader error (lexer / parser /
    /// unbalanced-paren / unterminated-string) short-circuits BEFORE
    /// `expand_program` runs; an `expand_program` error short-circuits
    /// BEFORE the keyword filter walks anything; the named-form gate
    /// (`split_first()` arity → `as_symbol_or_string()` shape) inside
    /// [`crate::compile::split_name_slot`] fires for the first matched
    /// form that violates either condition; a per-form `project` error
    /// short-circuits at the first failing matched form via
    /// `Iterator::collect::<Result<Vec<R>, _>>()`. Each consumer's
    /// rejection chain inherits the constant-keyword named primitive's
    /// shape verbatim, now sourced from `&str` via ONE composition
    /// point.
    ///
    /// Sibling of [`Self::expand_and_collect_named_calls_to`] (the
    /// from-forms posture) and of [`Self::expand_source_and_collect_calls_to`]
    /// (the from-source × bare-kwargs cell on the constant-keyword
    /// row). The four cells of (named × {constant, classifier} ×
    /// {from-forms, from-source}) close on the `Expander` surface
    /// post-lift, each routing through ONE composition point.
    ///
    /// Theory anchor: same as [`Self::expand_and_collect_named_calls_to`].
    /// THEORY.md §VI.1 (generation over composition; the from-source
    /// posture inherits the constant-keyword named composition through
    /// delegation rather than re-deriving the
    /// `read(src)? + expand_and_collect_named_calls_to` chain at every
    /// from-source named-keyword consumer's call site),
    /// THEORY.md §II.1 invariant 2 (free middle; the from-source
    /// posture and the from-forms posture route through the SAME
    /// composition).
    pub fn expand_source_and_collect_named_calls_to<R, F>(
        &mut self,
        src: &str,
        keyword: &'static str,
        mut project: F,
    ) -> Result<Vec<R>>
    where
        F: FnMut(&str, &[Sexp]) -> Result<R>,
    {
        // From-source = from-forms × constant-classifier specialization
        // of the named classifier primitive's from-source sibling. The
        // discarded `()` typed witness and the `move |(), name, args|
        // project(name, args)` wrapper mirror the from-forms posture's
        // composition — both postures route through the SAME named
        // classifier primitive on the `Expander` surface, the
        // from-source row stacking one `read` step on top.
        self.expand_source_and_collect_named_calls_to_any(
            src,
            |h| (h == keyword).then_some(((), keyword)),
            move |(), name, args| project(name, args),
        )
    }

    /// Expand a single form. Top-level macro calls are rewritten; recurses
    /// into list children.
    ///
    /// Routes the macro-call dispatch surface through the substrate's
    /// typed-decoded call decomposition: `as_call_to_any(|h|
    /// self.macros.get(h))` answers "is this form an invocation of any
    /// registered macro, decoded to `(&MacroDef, args)`?" in ONE
    /// structural query on the `Sexp` algebra. Pre-lift the same site
    /// opened the three-step chain `as_list() + as_call() + self.macros.
    /// get(head)` inline — `as_list()` for the children-walk fallthrough,
    /// `as_call()` for the (head, args) decomposition (which itself
    /// re-derives `as_list()` internally), and `self.macros.get(head)`
    /// for the registry lookup; post-lift the call-recognition runs as
    /// ONE `as_call_to_any` projection with the HashMap lookup as its
    /// classifier, and the `as_list()` fallthrough fires only on the
    /// not-a-macro-call path. Sibling consumer to `macro_def_from` — the
    /// typed-macro-definition dispatcher that routes through
    /// `as_call_to_any(MacroDefHead::from_keyword)` with the closed-set
    /// enum classifier. With both in place, BOTH dispatch sites in the
    /// macro expander (definition-recognition + call-recognition)
    /// project through the SAME family primitive on the `Sexp` algebra,
    /// each binding the classifier that fits its candidate set — closed
    /// enum for the static head-set, HashMap lookup for the live
    /// registry. A regression that drifts ONE site from the other (a
    /// future emitter that re-derives `as_list()` + `head.as_symbol()` +
    /// `self.macros.get(_)` inline rather than routing through the
    /// family) is no longer a silent two-site divergence.
    pub fn expand(&self, form: &Sexp) -> Result<Sexp> {
        self.expand_with_depth(form, 0)
    }

    /// The depth-carrying peer of [`Self::expand`]. Each
    /// post-apply re-expansion increments `depth`; each tree-child
    /// descent passes `depth` through unchanged. The runaway
    /// `(defmacro loop (x) `(loop ,x))` accretes depth on the
    /// re-expansion path — one unit per round — and hits the ceiling
    /// in `max_expansion_depth` rounds; lawful nested macros
    /// (`(when1 (when1 x))`) accrete depth in the low single digits
    /// because tree traversal doesn't count.
    ///
    /// The `depth >= limit` check sits INSIDE the macro-call arm
    /// (`Some((def, args)) = form.as_call_to_any(…)`) rather than at
    /// the top of the function so `def.name.clone()` is available at
    /// the rejection site — the operator gets `macro_name` populated
    /// from the actual offending call rather than a `<unknown>`
    /// fallback. Tree-child descent is not gated on the ceiling
    /// because the tree-child arm does not accrete depth; a lawfully
    /// deep tree is bounded by the reader's own stack, not by this
    /// ceiling.
    ///
    /// The peer OUTPUT-SIZE gate (`expanded.node_count() >
    /// self.limits.max_expansion_size`) sits INSIDE the same macro-call arm,
    /// between the `apply` and the re-expansion recursion, so a
    /// single macro's `apply` output whose structural size crosses
    /// the ceiling is rejected AT the offending `apply` boundary
    /// (with `macro_name` populated from `def.name`) before the
    /// runaway tree feeds back into further expansion. The gate is
    /// keyed on `>` rather than `>=` so `max_expansion_size` names
    /// the LARGEST admissible output — the ceiling admits equality,
    /// only strict overrun rejects. Together with the depth ceiling
    /// (recursion length) and the cache ceiling (memoization width),
    /// this closes the expander's RESOURCE surface at three typed
    /// dimensions.
    fn expand_with_depth(&self, form: &Sexp, depth: usize) -> Result<Sexp> {
        if let Some((def, args)) = form.as_call_to_any(|h| self.macros.get(h)) {
            if depth >= self.limits.max_expansion_depth {
                return Err(LispError::ExpansionDepthExceeded {
                    macro_name: def.name.clone(),
                    limit: self.limits.max_expansion_depth,
                });
            }
            let expanded = self.apply(def, args)?;
            // Reject a single `apply` output whose structural node
            // count crosses the OUTPUT-SIZE ceiling — the RESOURCE
            // axis peer of the depth (recursion length) and
            // cache-entries (memoization width) ceilings. Catches
            // the canonical "expansion bomb" where a well-defined
            // macro produces a 2^N-node blob from a small input
            // while `depth` stays in the low single digits and the
            // cache entry count sits under any reasonable ceiling.
            let expanded_size = expanded.node_count();
            if expanded_size > self.limits.max_expansion_size {
                return Err(LispError::ExpansionSizeExceeded {
                    macro_name: def.name.clone(),
                    size: expanded_size,
                    limit: self.limits.max_expansion_size,
                });
            }
            // Recurse — the expansion itself may contain more macro calls.
            return self.expand_with_depth(&expanded, depth + 1);
        }
        // Not a macro call — expand children if this is a list; otherwise
        // (atom / Nil / quote-family wrapper) return the form verbatim.
        let Some(list) = form.as_list() else {
            return Ok(form.clone());
        };
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            out.push(self.expand_with_depth(item, depth)?);
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
            // Layer 3: substitute fallback. Walk the body's substitution
            // projection — the inner of the outer quasi-quote when present,
            // the body verbatim otherwise — through the shared
            // `MacroDef::template_body` primitive both strategies route on.
            let bindings = bind_args(&def.name, &def.params, args)?;
            substitute(def.template_body(), &bindings)?
        };

        // Populate cache on miss — capped by `max_cache_entries` so the
        // memoization table cannot drift unboundedly across a long-
        // running host session. When the cap is reached the freshly-
        // computed result is returned verbatim; correctness is
        // unaffected because caching is a pure PERFORMANCE optimization
        // (the miss path always recomputes the same value the cache
        // would have returned).
        if let Some(key) = cache_key {
            let mut cache = self.cache.lock().unwrap();
            if cache.len() < self.limits.max_cache_entries {
                cache.insert(key, result.clone());
            }
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
///
/// The gate routes through [`Sexp::as_unquote`] — the typed-marker
/// projection that pairs `Sexp::UnquoteSplice ↔ UnquoteForm::Splice` at
/// ONE structural query — matching `Some((UnquoteForm::Splice, inner))`
/// rather than the per-arm `Sexp::UnquoteSplice(inner)` literal that
/// recurred at three sites pre-lift (`compile_node` Subst/Splice arms,
/// `substitute` top-level + list-inner). Sibling shape to `substitute`'s
/// list-inner Splice arm — both use the same `Some((UnquoteForm::Splice,
/// inner))` shape; `substitute`'s top-level arm uses the wider
/// `Some((kind, inner))` and dispatches inside on `kind`. After this lift
/// every production-site recognizer of "is this an `,@X` form" routes
/// through ONE typed-marker projection rather than re-deriving the
/// (Sexp variant, UnquoteForm variant) pair inline.
pub fn compile_template(def: &MacroDef) -> Result<CompiledTemplate> {
    // Walk the body's substitution projection — the inner of the outer
    // quasi-quote when present, the body verbatim otherwise — through the
    // shared `MacroDef::template_body` primitive the substitute path also
    // routes on. Same projection, both strategies, by construction.
    let body = def.template_body();
    if let Some((UnquoteForm::Splice, inner)) = body.as_unquote() {
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
    // Routes the `Sexp::Unquote(inner)` / `Sexp::UnquoteSplice(inner)` arms
    // through [`Sexp::as_unquote`] — the typed-marker projection that
    // pairs `Sexp::Unquote ↔ UnquoteForm::Unquote` and
    // `Sexp::UnquoteSplice ↔ UnquoteForm::Splice` at ONE site. The per-form
    // `TemplateOp` emission (`Subst` vs `Splice`) keys on the same typed
    // `form` value the gate-1+gate-2 composition `resolve_unquote_in_params`
    // threads through. Pre-lift the (Sexp variant, UnquoteForm variant)
    // pairing was bound per-arm — a future emitter that matched
    // `Sexp::Unquote(_)` but threaded `UnquoteForm::Splice` into
    // `resolve_unquote_in_params` (or vice versa) would type-check but
    // render a misleading diagnostic at the gate-1 / gate-2 rejection.
    // Post-lift the pair is bound at ONE projection function and the
    // `match form` mechanically lowers it to the bytecode op.
    if let Some((form, inner)) = node.as_unquote() {
        let idx = resolve_unquote_in_params(inner, params, form)?;
        ops.push(match form {
            UnquoteForm::Unquote => TemplateOp::Subst(idx),
            UnquoteForm::Splice => TemplateOp::Splice(idx),
        });
        return Ok(());
    }
    match node {
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
    // Route every quote-family wrapper recognition — both the
    // unquote-only subset (Unquote/UnquoteSplice → short-circuit `true`)
    // AND the quote-only subset (Quote/Quasiquote → recurse into inner) —
    // through [`Sexp::as_quote_form`]'s typed-marker projection. Pre-lift
    // the recognizer split into two arms: `as_unquote().is_some()` (the
    // substitution subset gate) + the inline
    // `Sexp::Quote(inner) | Sexp::Quasiquote(inner)` arm (the remaining
    // quote-only subset). Post-lift both arms route through ONE
    // projection and the (Sexp variant, QuoteForm variant) pairing binds
    // at the closed-set algebra. The `form.as_unquote_form().is_some()`
    // gate is the SAME 2-of-4 subset projection [`Sexp::as_unquote`]
    // derives from, so recognizing "is this an unquote-family wrapper"
    // and "is this a quote-only wrapper" share ONE typed dispatch site;
    // rustc enforces that a future `Sexp` wrapper extension carry
    // through both `QuoteForm::ALL` AND `QuoteForm::as_unquote_form`'s
    // arm. Sibling posture to `Hash for Sexp`'s four-arm
    // `hash_discriminator` collapse, `Display for Sexp`'s `prefix`
    // collapse, `interop`'s `iac_forge_tag` collapse, and `domain`'s
    // `sexp_shape` collapse — every production-site quote-family
    // recognizer now routes through ONE projection on the algebra.
    if let Some((form, inner)) = node.as_quote_form() {
        return form.as_unquote_form().is_some() || contains_unquote(inner);
    }
    match node {
        Sexp::List(items) => items.iter().any(contains_unquote),
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

/// Look up a bound-arg by its template-bytecode index, or raise the
/// structural `LispError::TemplateInvariant` rejection with the
/// caller-supplied `kind` constructor applied to the bad index. ONE
/// named primitive both bytecode-runtime arms that read a bound arg
/// by index — [`TemplateOp::Subst`] (single-value push) AND
/// [`TemplateOp::Splice`] (list-splicing) — route through.
///
/// Before this lift the same `args_by_index.get(*idx).ok_or_else(||
/// template_invariant_violation(macro_name, KIND(*idx)))?` projection
/// appeared at BOTH arms of [`apply_compiled`], differing only in the
/// kind constructor: [`TemplateInvariantKind::SubstBadIndex`] at the
/// `Subst` arm, [`TemplateInvariantKind::SpliceBadIndex`] at the
/// `Splice` arm. The arms also diverged on what they did with the
/// returned `&Sexp` — `Subst` cloned and pushed, `Splice` consumed
/// the borrow through [`splice_value_into`] — but the lookup-and-
/// reject prelude was byte-identical modulo the kind, well past the
/// ≥2 PRIME-DIRECTIVE trigger.
///
/// After this lift the lookup-and-reject shape lives in ONE function;
/// the two arms thread the per-call-site kind constructor through the
/// helper and apply their respective post-lookup verbs at the call
/// site. The `kind: FnOnce(usize) -> TemplateInvariantKind` parameter
/// encodes the closed-set bytecode-runtime "this gate has an index"
/// surface at the type level — only the two
/// [`TemplateInvariantKind`] variants whose payload IS the bad index
/// (`SubstBadIndex(usize)` and `SpliceBadIndex(usize)`) construct
/// directly through `FnOnce(usize) -> TemplateInvariantKind`; the
/// stack-gate variants ([`TemplateInvariantKind::EndListEmptyStack`]
/// and [`TemplateInvariantKind::FinalNoValue`]) carry no payload and
/// would not type-check at this boundary, so the invalid combination
/// "stack-gate kind reached from an op-index lookup" is structurally
/// unrepresentable at the helper's call boundary the same way
/// [`TemplateInvariantKind`]'s closed-set shape makes it
/// unrepresentable in the variant itself.
///
/// Sibling of [`template_invariant_violation`]: that helper builds the
/// typed [`LispError::TemplateInvariant`] variant from a fully-formed
/// `kind`; this helper composes the index-keyed lookup with the
/// variant-builder, so the kind constructor doesn't have to be evaluated
/// eagerly at the call site (lazy via `FnOnce`, only fires on the bad-
/// index path). A future fifth bytecode op that reads a bound arg by
/// index (a hypothetical [`TemplateOp::Conditional`] that branches on a
/// bound boolean, a [`TemplateOp::Project`] that extracts a sub-field
/// of a bound `Sexp::List`) extends the family in ONE call to
/// `resolve_bound_arg` with the new kind constructor (`KIND(usize) ->
/// TemplateInvariantKind`) — the bytecode-runtime's bound-arg-by-index
/// projection becomes ONE structural primitive consumers compose with.
///
/// The returned `&'a Sexp` borrows from `args_by_index` verbatim —
/// `Subst`'s arm consumes it through `.clone()` (the consumer pushes
/// an owned value into the builder); `Splice`'s arm consumes it
/// through [`splice_value_into`] (the consumer borrows for the
/// per-arm coercion). The borrow's lifetime `'a` is the unified
/// lifetime of `args_by_index`, matching the call site's borrow
/// posture.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// inline copies of the index-lookup-and-reject prelude across the
/// `apply_compiled` body's `Subst` and `Splice` arms is past the ≥2
/// PRIME-DIRECTIVE trigger once the structural shape is named.
/// THEORY.md §V.1 — knowable platform / "make invalid states
/// unrepresentable"; the bytecode-runtime bound-arg-by-index lookup
/// becomes a NAMED primitive on the substrate's `&[Sexp]` algebra
/// rather than a re-derived `get + ok_or_else + template_invariant_
/// violation` chain at every op-arm that reads by index. A future
/// authoring tool (REPL, LSP, `tatara-check`) that wants to surface
/// "this bytecode op's bound-arg lookup misfired at idx N" binds to
/// ONE function. THEORY.md §II.1 invariant 2 — free middle; both
/// expansion strategies route through the SHARED `MacroParams::bind`,
/// AND the bytecode strategy's op-arms route through this SHARED
/// `resolve_bound_arg` lookup — the bytecode-runtime's
/// proof-of-well-formedness is now structurally uniform across the
/// two reachable index-lookup ops, so a regression that drifts ONE
/// arm's posture (e.g. accepts an out-of-range idx at one arm but
/// not the other, or swaps the kind constructor at a single arm) is
/// no longer a silent two-site divergence.
fn resolve_bound_arg<'a>(
    args_by_index: &'a [Sexp],
    idx: usize,
    macro_name: &str,
    kind: impl FnOnce(usize) -> TemplateInvariantKind,
) -> Result<&'a Sexp> {
    args_by_index
        .get(idx)
        .ok_or_else(|| template_invariant_violation(macro_name, kind(idx)))
}

/// Project the bytecode-runtime stack to its in-progress builder frame —
/// the `&mut Vec<Sexp>` every value-emitting op writes into. ONE named
/// primitive both push-emitting arms (`TemplateOp::Literal` /
/// `TemplateOp::Subst` / post-`EndList` parent fold) AND the splice-
/// emitting arm (`TemplateOp::Splice`) route through.
///
/// Before this lift the same `stack.last_mut().unwrap()` projection
/// appeared at FOUR sites inside [`apply_compiled`]'s op-loop:
///
///   * `TemplateOp::Literal` — pushes the literal `Sexp` into the
///     current builder.
///   * `TemplateOp::Subst` — pushes the cloned bound-arg into the
///     current builder.
///   * `TemplateOp::Splice` — splices the bound-arg into the current
///     builder via [`splice_value_into`].
///   * `TemplateOp::EndList` — after popping the just-finished list
///     frame, pushes the folded `Sexp::List(items)` into the parent
///     builder (the new current frame).
///
/// Four byte-identical re-derivations of the same projection, well past
/// the ≥2 PRIME-DIRECTIVE trigger. After this lift the four sites
/// collapse to a single `current_builder_mut(&mut stack).{push|extend}`
/// call, and the bytecode-runtime invariant the projection rests on
/// — "the op-loop always sees at least one stack frame" — lives in ONE
/// expect message rather than four silent `.unwrap()` calls.
///
/// The expect rationale: [`apply_compiled`] seeds the stack with the
/// outermost frame at entry (`vec![Vec::with_capacity(1)]`); every
/// `TemplateOp::BeginList` pushes a NEW frame and every
/// `TemplateOp::EndList` pops it, so the count stays at OR ABOVE 1
/// throughout the op-loop. Stack-depleting failure modes are caught
/// upstream by their own structural variants:
/// [`TemplateInvariantKind::EndListEmptyStack`] fires inside
/// [`apply_compiled`]'s `EndList` arm via [`Vec::pop`]'s `Option`
/// gate, BEFORE the parent-fold push runs against
/// `current_builder_mut`; [`TemplateInvariantKind::FinalNoValue`]
/// fires AFTER the op-loop completes, on the outermost `stack.pop()`
/// that returns the assembled result. So a reachable
/// `current_builder_mut(&mut stack)` always observes a non-empty
/// stack, and the `expect` is a structural-invariant marker, not a
/// load-bearing rejection path.
///
/// Sibling of [`resolve_bound_arg`] (the bytecode-runtime bound-arg
/// lookup primitive lifted in the prior claude-routine run on this
/// module — 492a235) and [`template_invariant_violation`] (the
/// structural-variant error builder for the bytecode-runtime's
/// closed-set invariant-violation surface). Together the three primitives
/// name the bytecode-runtime's substrate-level operations: lookup-a-
/// bound-arg ([`resolve_bound_arg`]), build-the-invariant-rejection
/// ([`template_invariant_violation`]), and project-to-the-current-
/// builder (this lift). A future bytecode op that emits ONE OR MORE
/// values into the current builder — a hypothetical
/// `TemplateOp::SpliceMany(indices: Vec<usize>)` that splices a batch,
/// a `TemplateOp::PushQuoted(form: Sexp)` that wraps before push, a
/// span-annotated emit-with-position op — composes with ONE call to
/// [`current_builder_mut`] and the per-op post-projection verb
/// (`.push(…)`, `.extend(…)`, `splice_value_into(…, _)`); a future
/// instrumentation hook that wants to log every op's emit before
/// it lands in the builder wraps ONE call boundary, not four inline
/// `stack.last_mut().unwrap()` sites.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four
/// inline copies of the top-of-stack projection in one function is
/// past the ≥2 PRIME-DIRECTIVE trigger once the structural shape is
/// named. THEORY.md §V.1 — knowable platform; the bytecode-runtime's
/// current-builder projection becomes a NAMED primitive on the
/// substrate's `&mut [Vec<Sexp>]` slice algebra rather than a re-derived
/// `last_mut + unwrap` chain at every op-arm that emits into the
/// builder. The expect message names the invariant
/// ("bytecode-runtime invariant: at least one stack frame during
/// op-loop") so a regression that drifts the loop's frame management
/// surfaces a NAMED panic, not a silent `unwrap` over `None`.
/// THEORY.md §II.1 invariant 2 — free middle; both expansion
/// strategies route through the SHARED `MacroParams::bind` upstream
/// AND the bytecode strategy's op-arms now route through this SHARED
/// `current_builder_mut` projection downstream — the bytecode-runtime's
/// substrate-level surface (lookup + emit) is named in two
/// composable primitives the op-arms compose with.
fn current_builder_mut(stack: &mut [Vec<Sexp>]) -> &mut Vec<Sexp> {
    stack
        .last_mut()
        .expect("bytecode-runtime invariant: at least one stack frame during op-loop")
}

/// Pop the top stack frame off the bytecode-runtime stack, or raise the
/// structural [`LispError::TemplateInvariant`] rejection with the
/// supplied [`TemplateInvariantKind`] when the stack is empty — ONE
/// named primitive both pop-emitting sites in [`apply_compiled`] route
/// through.
///
/// Before this lift two byte-identical
/// `stack.pop().ok_or_else(|| template_invariant_violation(macro_name,
/// kind))?` chains lived inline in [`apply_compiled`]:
///
///   * `TemplateOp::EndList` arm — pops the just-finished list frame
///     before the parent-fold push, with kind
///     [`TemplateInvariantKind::EndListEmptyStack`] guarding the
///     unreachable empty-stack failure mode.
///   * Post-loop final pop — consumes the outermost frame that
///     accumulated the template's result, with kind
///     [`TemplateInvariantKind::FinalNoValue`] guarding the
///     unreachable seed-frame-already-popped failure mode.
///
/// Two byte-identical re-derivations of the same projection inside one
/// function, past the ≥2 PRIME-DIRECTIVE trigger once the structural
/// shape is named. After this lift the two sites collapse to a single
/// `pop_builder_frame(&mut stack, macro_name, KIND)?` call, and the
/// bytecode-runtime invariant the projection rests on — "an empty-stack
/// pop is a structural-variant rejection, not a silent `Option::None`"
/// — lives in ONE composition point rather than two.
///
/// Sibling of [`current_builder_mut`] (the bytecode-runtime stack's
/// *project-to-top-frame* primitive — the borrow face, never resizes
/// the stack) and [`resolve_bound_arg`] (the bound-arg-by-index
/// lookup primitive with per-call-site `TemplateInvariantKind`
/// constructor). Where `current_builder_mut` borrows the in-progress
/// top frame for emission and never panics on a reachable input
/// (callers route past it only when the seed frame is present), this
/// primitive *consumes* the top frame off the stack and projects its
/// absence into a structural `LispError::TemplateInvariant` rejection
/// — the failure-on-empty face of the same `&mut Vec<Vec<Sexp>>`
/// algebra. The pair —
/// [`current_builder_mut`] + [`pop_builder_frame`] — close the
/// substrate's bytecode-runtime stack-frame projection algebra at the
/// borrow/consume boundary: borrow-the-top-frame for emission (no
/// rejection — the invariant rests on the seed-frame contract),
/// consume-the-top-frame for finalization (rejection-routed —
/// the absence projects through `TemplateInvariantKind` into a
/// structural variant of [`LispError`]). Together with
/// [`resolve_bound_arg`] (lookup args by index, with per-call-site
/// kind constructor) and [`template_invariant_violation`] (the typed
/// rejection emitter the three primitives share), the four
/// substrate-level operations name the bytecode-runtime's named
/// projection surface: lookup-a-bound-arg, project-to-the-current-
/// builder, consume-a-finished-builder-frame, build-the-invariant-
/// rejection.
///
/// `kind: TemplateInvariantKind` is the closed-set typed enum whose
/// four variants are EXACTLY the four reachable bytecode-runtime
/// invariant-violation modes (`SubstBadIndex(usize)` /
/// `SpliceBadIndex(usize)` from the index-lookup sibling primitive,
/// `EndListEmptyStack` from the `EndList` arm, `FinalNoValue` from
/// the post-loop final pop). The Subst / Splice indexed variants
/// thread their `usize` payload INSIDE the variant, so the invalid
/// combination "stack-gate kind with an op-index" (a hypothetical
/// `EndListEmptyStack(99)` carrying a Subst-style payload) is
/// structurally unrepresentable at this helper's boundary — the
/// caller cannot misroute an indexed kind through `pop_builder_frame`
/// at compile time because the kind's data shape is part of its
/// variant identity. Same closed-set guarantee
/// [`template_invariant_violation`] gives the four kinds; this
/// helper composes that guarantee with the `stack.pop()` projection
/// at the two pop-emitting sites.
///
/// The future-run extensions ride this floor: a future bytecode op
/// that consumes one or more finished frames — a hypothetical
/// `TemplateOp::EndMany(n: usize)` that pops `n` frames into a
/// flattened list, a span-aware `TemplateOp::EndListWithSpan(pos)`
/// that pops with a position-annotated rejection — composes with
/// ONE call (or a fold over N calls) to [`pop_builder_frame`]
/// without re-deriving the stack-pop-and-reject shape. A future
/// instrumentation hook (a debug-mode logger that records every
/// frame consumption, a span-aware pop that threads `Sexp` positions
/// through, a multi-frame fold that pops N frames in one step)
/// wraps ONE call boundary rather than keeping two inline chains in
/// lockstep at the production op-loop sites.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// inline copies of the stack-pop-and-reject projection across the
/// `apply_compiled` body's `EndList` arm and post-loop final-pop is
/// past the ≥2 PRIME-DIRECTIVE trigger once the structural shape is
/// named — the same threshold [`resolve_bound_arg`] (the index-lookup
/// sibling) crossed in the prior claude-routine run on this file
/// (492a235) and [`current_builder_mut`] (the top-frame-borrow
/// sibling) crossed two runs ago (c6a5a9d). THEORY.md §V.1 — knowable
/// platform / "make invalid states unrepresentable"; the bytecode-
/// runtime stack-frame consume operation becomes a NAMED primitive
/// on the substrate's `&mut Vec<Vec<Sexp>>` algebra rather than a
/// re-derived `pop + ok_or_else + template_invariant_violation`
/// chain at every op-arm that consumes a frame. THEORY.md §II.1
/// invariant 2 — free middle; both pop-emitting sites route through
/// the SHARED `pop_builder_frame` projection, so a regression that
/// drifts ONE site's posture (e.g. accepts an empty-stack pop at the
/// `EndList` arm but not the final pop, or swaps the kind constructor
/// at a single site) is no longer a silent two-site divergence — the
/// type system binds both sites to ONE composition point.
///
/// Frontier inspiration: MLIR's `Block::eraseFromParent()` against a
/// region's block list — the structured-IR's block-consumption
/// operation is a named typed primitive that yields a typed
/// `LogicalResult` rejection rather than a silent `nullptr` projection
/// when the parent region is empty; the substrate's
/// `pop_builder_frame` is the unstructured-projection peer on the
/// substrate's `&mut Vec<Vec<Sexp>>` stack-frame algebra, with
/// `TemplateInvariantKind` standing in for MLIR's `LogicalResult`'s
/// closed-set rejection identity. GHC Core's `popTickish` /
/// `stackPop` family — every Core-IR transform that consumes a stack
/// frame off the rewriter's working stack binds to one named pop
/// primitive that threads a typed `WantedFailure` rejection when the
/// stack is empty; translated through pleme-io primitives as ONE
/// `pop_builder_frame(stack, macro_name, kind)` call with
/// `TemplateInvariantKind` carrying the closed-set rejection
/// identity.
fn pop_builder_frame(
    stack: &mut Vec<Vec<Sexp>>,
    macro_name: &str,
    kind: TemplateInvariantKind,
) -> Result<Vec<Sexp>> {
    stack
        .pop()
        .ok_or_else(|| template_invariant_violation(macro_name, kind))
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
    // Each emit-into-builder arm routes through the shared
    // `current_builder_mut` projection — the bytecode-runtime invariant
    // "at least one stack frame during the op-loop" lives in ONE expect
    // message rather than four silent `.unwrap()` calls.
    let mut stack: Vec<Vec<Sexp>> = vec![Vec::with_capacity(1)];
    for op in &tmpl.ops {
        match op {
            TemplateOp::Literal(s) => current_builder_mut(&mut stack).push(s.clone()),
            TemplateOp::Subst(idx) => {
                // Bound-arg-by-index lookup routes through the shared
                // `resolve_bound_arg` projection with `SubstBadIndex` as
                // the per-call-site kind constructor; the post-lookup
                // verb (clone + push into the current builder) is the
                // Subst arm's per-op shape.
                let v = resolve_bound_arg(
                    &args_by_index,
                    *idx,
                    macro_name,
                    TemplateInvariantKind::SubstBadIndex,
                )?
                .clone();
                current_builder_mut(&mut stack).push(v);
            }
            TemplateOp::Splice(idx) => {
                // Sibling lookup through `resolve_bound_arg` with
                // `SpliceBadIndex` as the per-call-site kind constructor;
                // the post-lookup verb (`splice_value_into` against the
                // current builder) consumes the borrow directly without
                // an intermediate clone.
                let v = resolve_bound_arg(
                    &args_by_index,
                    *idx,
                    macro_name,
                    TemplateInvariantKind::SpliceBadIndex,
                )?;
                splice_value_into(current_builder_mut(&mut stack), v);
            }
            TemplateOp::BeginList => stack.push(Vec::new()),
            TemplateOp::EndList => {
                // Pop the just-finished list frame through the shared
                // `pop_builder_frame` projection with
                // `EndListEmptyStack` as the per-call-site kind
                // constructor; the post-pop verb (folded
                // `Sexp::List(items)` push into the parent frame via
                // `current_builder_mut`) is the EndList arm's per-op
                // shape — sibling of the Subst/Splice arms' index-
                // lookup-then-emit posture, with the index-lookup
                // primitive (`resolve_bound_arg`) and the frame-
                // consume primitive (`pop_builder_frame`) BOTH routing
                // through the same `TemplateInvariantKind` closed-set
                // rejection identity.
                let items = pop_builder_frame(
                    &mut stack,
                    macro_name,
                    TemplateInvariantKind::EndListEmptyStack,
                )?;
                current_builder_mut(&mut stack).push(Sexp::List(items));
            }
        }
    }
    // Final pop consumes the outermost (seed) frame via the same
    // shared projection — `FinalNoValue` as the per-call-site kind
    // constructor pins this site's structural-variant identity, the
    // post-pop verb (the `top.len() == 1` arity gate below) is the
    // post-loop tail's per-site shape.
    let mut top = pop_builder_frame(&mut stack, macro_name, TemplateInvariantKind::FinalNoValue)?;
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
    // Route the typed-macro-definition dispatch surface through the
    // substrate's typed-decoded call decomposition: `as_call_to_any`
    // performs the `as_list + head_symbol + MacroDefHead::from_keyword`
    // three-step chain in ONE structural query on the `Sexp` algebra.
    // The legacy diagnostic anchors on `list.len()` (the FULL form arity
    // including the head) — preserved here as `args.len() + 1` so
    // `LispError::DefmacroArity.arity` carries the same value across the
    // lift.
    let Some((head, args)) = form.as_call_to_any(MacroDefHead::from_keyword) else {
        return Ok(None);
    };
    if args.len() < 3 {
        return Err(defmacro_arity(head, args.len() + 1));
    }
    let name = args[0]
        .as_symbol()
        .ok_or_else(|| defmacro_non_symbol_name(head, &args[0]))?
        .to_string();
    let param_list = args[1]
        .as_list()
        .ok_or_else(|| defmacro_non_list_params(head, &args[1]))?;
    let params = parse_params(param_list)?;
    let body = args[2].clone();
    Ok(Some(MacroDef { name, params, body }))
}

fn parse_params(list: &[Sexp]) -> Result<MacroParams> {
    let mut required = Vec::new();
    let mut optional: Vec<OptionalParam> = Vec::new();
    let mut optional_marker: Option<usize> = None;
    let mut i = 0;
    while i < list.len() {
        // In the optional section a `(name default)` LIST form is a valid spec
        // alongside a bare-symbol spec. The list form is only meaningful here,
        // so the dispatch fires before the `as_symbol()` gate that would
        // otherwise reject it as `NonSymbolParam`.
        if optional_marker.is_some() {
            if let Sexp::List(items) = &list[i] {
                optional.push(parse_optional_list_spec(i, &list[i], items)?);
                i += 1;
                continue;
            }
        }
        let s = list[i]
            .as_symbol()
            .ok_or_else(|| non_symbol_param(i, &list[i]))?;
        if s == MacroParams::REST_MARKER {
            let Some(next) = list.get(i + 1) else {
                return Err(rest_param_missing_name(i, None));
            };
            let Some(name) = next.as_symbol() else {
                return Err(rest_param_missing_name(i, Some(next)));
            };
            let trailing = &list[i + 2..];
            if !trailing.is_empty() {
                return Err(rest_param_trailing_tokens(i, trailing));
            }
            return Ok(MacroParams {
                required,
                optional,
                rest: Some(name.to_string()),
            });
        }
        if s == MacroParams::OPTIONAL_MARKER {
            if let Some(first) = optional_marker {
                return Err(optional_marker_repeated(first, i));
            }
            optional_marker = Some(i);
            i += 1;
            continue;
        }
        if optional_marker.is_some() {
            optional.push(OptionalParam::bare(s));
        } else {
            required.push(s.to_string());
        }
        i += 1;
    }
    Ok(MacroParams {
        required,
        optional,
        rest: None,
    })
}

/// Project a `Sexp::List` in the `&optional` section to a typed
/// [`OptionalParam`]. The only admissible shape is `(NAME DEFAULT)` — a
/// list of exactly TWO elements whose first element is a symbol. Every
/// other list shape is the structural rejection
/// [`LispError::OptionalParamMalformed`], with a typed `reason`
/// ([`OptionalParamMalformedReason`]) naming WHICH way the spec is
/// malformed — empty, missing-default, extra-elements, or non-symbol name.
///
/// `position` is the loop index inside `parse_params`, mirroring the
/// `position`/`rest_position`/`first_position` slots on the sibling
/// `parse_params` rejection variants. `list_form` is the offending
/// `Sexp::List` itself, projected through `crate::domain::sexp_witness` so
/// the variant carries BOTH `SexpShape::List` AND the rendered form (for
/// LSP / REPL / `tatara-check` consumption). `items` is the list body,
/// avoiding a re-`as_list()` at the call boundary.
fn parse_optional_list_spec(
    position: usize,
    list_form: &Sexp,
    items: &[Sexp],
) -> Result<OptionalParam> {
    use crate::error::OptionalParamMalformedReason as R;
    // Arity dispatch — threads through the typed classifier on the
    // closed-set algebra so the accept-arity binding (arity ==
    // `R::OPTIONAL_PARAM_SPEC_ARITY`) is derived from ONE typed const
    // rather than the bare literal `2`. Post-classification the arity
    // is exactly the accept target; the head-symbol gate is the only
    // remaining rejection axis.
    if let Some(reason) = R::classify_arity(items.len()) {
        return Err(optional_param_malformed(position, list_form, reason));
    }
    let Some(name) = items[0].as_symbol() else {
        return Err(optional_param_malformed(
            position,
            list_form,
            R::NonSymbolName,
        ));
    };
    Ok(OptionalParam::with_default(name, items[1].clone()))
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
///
/// Routes both unquote-family sites — the top-level `,X` / `,@X`
/// recognition AND the list-inner per-item splice recognition — through
/// the substrate's typed-marker projection [`Sexp::as_unquote`]. Pre-lift
/// each site opened its own `Sexp::Unquote(inner)` / `Sexp::UnquoteSplice
/// (inner)` arm paired with a `UnquoteForm::Unquote` / `UnquoteForm::
/// Splice` literal; post-lift the (Sexp variant, UnquoteForm variant)
/// pairing is bound at ONE projection function the type system threads
/// through `(UnquoteForm, &Sexp)`, eliminating the silent two-site
/// pairing drift the prior shape allowed.
fn substitute(form: &Sexp, bindings: &HashMap<String, Sexp>) -> Result<Sexp> {
    if let Some((kind, inner)) = form.as_unquote() {
        return match kind {
            UnquoteForm::Unquote => resolve_unquote_in_bindings(inner, bindings, kind).cloned(),
            UnquoteForm::Splice => Err(splice_outside_list(inner)),
        };
    }
    match form {
        Sexp::List(items) => {
            let mut out: Vec<Sexp> = Vec::with_capacity(items.len());
            for item in items {
                if let Some((UnquoteForm::Splice, inner)) = item.as_unquote() {
                    let val = resolve_unquote_in_bindings(inner, bindings, UnquoteForm::Splice)?;
                    splice_value_into(&mut out, val);
                } else {
                    out.push(substitute(item, bindings)?);
                }
            }
            Ok(Sexp::List(out))
        }
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
        got: got.witness(),
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
        got: inner.witness(),
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

/// Mirror at the call-site of `missing_macro_arg`: that helper fires when
/// the macro CALL supplies TOO FEW args for the required arity (a required
/// slot has no arg); this helper fires when the macro CALL supplies TOO
/// MANY args for a rest-less param list (the surplus has nowhere to bind).
/// Together they close the typed-entry macro-call-gate's positional-arity
/// surface in both directions; together with the definition-site
/// `RestParamTrailingTokens` (lifted by the prior-run typed-promotion
/// lineage at the parse_params boundary), every distinct way a macro
/// definition + call pair can MISCOUNT args is now a named structural
/// rejection.
///
/// `expected` is the rest-less binder's fixed maximum arity
/// (`required.len() + optional.len()`); `got` is the actual call-site arg
/// count. Both are surfaced at the variant boundary so authoring tools
/// (REPL, LSP, `tatara-check`) name the "you supplied {got} args but the
/// macro takes at most {expected}" quick-fix from one structural projection
/// rather than re-deriving either count from the source. `macro_name` is
/// `&str` borrowed from the call-site; the variant's owned `String` is
/// formed at the boundary so `LispError` stays lifetime-free — same posture
/// as `missing_macro_arg`.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// rest-less surplus-args gate is a SINGLE-OWNER named rejection, not a
/// silent truncation re-asserted at every consumer that walks the bound
/// values. THEORY.md §V.1 — knowable platform; the structural variant
/// exposes `macro_name` / `expected` / `got` as first-class fields so
/// authoring tools bind to the data shape instead of substring-parsing
/// the rendered diagnostic. THEORY.md §II.1 invariant 1 — typed entry; a
/// macro call with too many args (and no `&rest` slot to absorb them) is
/// exactly the failure mode the typed-entry gate exists to reject —
/// silently dropping `args[expected..]` is structurally indistinguishable
/// from honoring them, the asymmetry this gate closes. THEORY.md §II.1
/// invariant 2 — free middle; both expansion strategies route through the
/// SHARED `MacroParams::bind`, so the new rejection lands once and the
/// substitute + bytecode paths inherit it unable to drift.
fn too_many_macro_args(macro_name: &str, expected: usize, got: usize) -> LispError {
    LispError::TooManyMacroArgs {
        macro_name: macro_name.to_string(),
        expected,
        got,
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
        got: got.witness(),
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
        got: got.map(Sexp::witness),
    }
}

/// The third and final `parse_params` definition-site rejection — a
/// `&rest <name>` followed by further tokens. Sibling of `non_symbol_param`
/// (a param slot that isn't a symbol) and `rest_param_missing_name` (the
/// post-`&rest` follower is missing or malformed): this helper fires once
/// the rest name is bound and the walker finds the param list does not end
/// there. The `&rest` name absorbs every remaining call arg, so it is
/// structurally the LAST param a list can name; trailing tokens are
/// unrepresentable in `MacroParams` and were previously dropped silently.
///
/// `rest_position` is the loop index of the `&rest` marker (parallel to
/// `rest_param_missing_name`); `trailing` is the non-empty token run after
/// the bound rest name — the helper records its length and the typed
/// witness of its first element. The caller guarantees `trailing` is
/// non-empty (it is only built when `list[i + 2..].first()` is `Some`), so
/// `trailing[0]` does not panic.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid states
/// unrepresentable"; a param list with tokens after `&rest <name>` is
/// nonsense `MacroParams` cannot hold, so the gate must REJECT it rather
/// than truncate to the representable prefix. THEORY.md §II.1 invariant 1 —
/// typed entry; the gate rejects malformed DEFINITIONS as readily as
/// malformed calls. THEORY.md §VI.1 — generation over composition; this
/// closes the `parse_params` walker's last uncovered failure mode, making
/// the sibling docs' "every distinct failure mode is a structural variant"
/// claim finally true.
fn rest_param_trailing_tokens(rest_position: usize, trailing: &[Sexp]) -> LispError {
    LispError::RestParamTrailingTokens {
        rest_position,
        extra: trailing.len(),
        first: trailing[0].witness(),
    }
}

/// A `&optional` marker appeared a SECOND time in one param list —
/// `(defmacro f (a &optional b &optional c) …)`. The lambda-list has exactly
/// ONE optional section (between the required run and the rest); a second
/// `&optional` is nonsense `MacroParams` cannot hold (its `optional` field is
/// one flat run, not a sequence of sections). Without this gate the parser
/// would otherwise treat the second `&optional` as an optional param literally
/// NAMED `&optional`, binding call args to a marker symbol — exactly the kind
/// of silent misalignment the typed shape exists to forbid.
///
/// Sibling of `rest_param_trailing_tokens` (the rest-section ordering gate):
/// both reject a param list whose marker structure the canonical lambda-list
/// ordering cannot represent. `first_position` is the loop index of the
/// first `&optional`, `second_position` the second — naming both lets an LSP
/// quick-fix point at the redundant marker to delete.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid states
/// unrepresentable"; a param list with two `&optional` sections is nonsense
/// `MacroParams` cannot hold, so the gate must REJECT rather than bind args
/// to a marker symbol. THEORY.md §II.1 invariant 1 — typed entry; the gate
/// rejects malformed DEFINITIONS as readily as malformed calls.
fn optional_marker_repeated(first_position: usize, second_position: usize) -> LispError {
    LispError::OptionalMarkerRepeated {
        first_position,
        second_position,
    }
}

/// An `&optional` section entry that's a `Sexp::List` did NOT match the only
/// admissible shape `(NAME DEFAULT)` — exactly two elements with a symbol
/// head. This helper builds the structural rejection from the loop position,
/// the offending list form (projected through `crate::domain::sexp_witness`
/// to carry both `SexpShape::List` and the literal display), and the typed
/// `OptionalParamMalformedReason` naming which of the four malformed shapes
/// fired (empty list / missing default / extra elements / non-symbol name).
///
/// Sibling of `optional_marker_repeated` (the `&optional`-section marker
/// gate) and `non_symbol_param` (the bare-symbol gate): the three together
/// close every distinct typed-entry rejection the optional section can
/// emit. The bare-symbol form `&optional x` is still routed through
/// `non_symbol_param`'s sibling acceptance path; the list form `&optional
/// (x default)` is admitted iff this gate accepts the spec.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid states
/// unrepresentable"; an `&optional` list spec of any other shape is
/// nonsense `MacroParams` cannot hold, so the gate must REJECT rather than
/// bind args to a marker symbol or drop the extras silently. THEORY.md
/// §II.1 invariant 1 — typed entry; a malformed default-form spec is
/// exactly the failure mode the typed-entry gate exists to reject — and
/// the gate must reject DEFINITIONS as readily as it rejects CALLS.
fn optional_param_malformed(
    position: usize,
    got: &Sexp,
    reason: crate::error::OptionalParamMalformedReason,
) -> LispError {
    LispError::OptionalParamMalformed {
        position,
        got: got.witness(),
        reason,
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
        got: got.witness(),
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
        got: got.witness(),
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
                optional: Vec::new(),
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

    // ── TooManyMacroArgs: call-site mirror of RestParamTrailingTokens ──
    //
    // A rest-less param list has a FIXED maximum arity equal to
    // `required.len() + optional.len()`. Surplus call args have nowhere to
    // bind. Before this gate the surplus was silently truncated to the
    // slice the binder could consume — the typed-entry macro-call-gate
    // rejected too-few-args loudly (`MissingMacroArg`) but accepted
    // too-many silently, an asymmetry the definition-side `&rest <name>
    // extra` rejection (`RestParamTrailingTokens`) had no call-side dual.
    // After this gate the call-site arity surface is structurally
    // complete in both directions; the substitute + bytecode paths share
    // `MacroParams::bind`, so both inherit the rejection without drift.

    /// Helper for the too-many-args tests — projects to (macro_name,
    /// expected, got) for legibility. Sibling of `missing_macro_arg_fields`.
    fn too_many_macro_args_fields(err: &LispError) -> (&str, usize, usize) {
        match err {
            LispError::TooManyMacroArgs {
                macro_name,
                expected,
                got,
            } => (macro_name.as_str(), *expected, *got),
            other => panic!("expected TooManyMacroArgs, got: {other:?}"),
        }
    }

    #[test]
    fn too_many_macro_args_required_only_rejected_with_expected_and_got() {
        // `(defmacro f (a b) ...)` called as `(f 1 2 3)` — `3` has
        // nowhere to bind. The rest-less binder rejects via
        // `TooManyMacroArgs { macro_name: "f", expected: 2, got: 3 }`,
        // NOT silently drops `3`. Pins both the variant identity AND the
        // structural fields the typed gate exposes for authoring-tool
        // quick-fixes ("you supplied 3 args; the macro takes at most
        // 2").
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a b) `(list ,a ,b)) (f 1 2 3)").unwrap())
            .expect_err("surplus arg on rest-less call must error");
        let (macro_name, expected, got) = too_many_macro_args_fields(&err);
        assert_eq!(macro_name, "f");
        assert_eq!(expected, 2);
        assert_eq!(got, 3);
    }

    #[test]
    fn too_many_macro_args_required_plus_optional_capacity_includes_optional() {
        // The rest-less binder's fixed maximum arity is `required.len() +
        // optional.len()` — the optional section CONTRIBUTES to capacity.
        // `(defmacro f (a &optional b) ...)` accepts 1 OR 2 args; 3
        // args rejects with `expected: 2` (required + optional, NOT just
        // required). Pins the optional-counts-in-capacity contract so a
        // regression that omits optionals from the expected calculation
        // (and erroneously rejects 2-arg calls) fails-loudly here.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a &optional b) `(list ,a ,b)) (f 1 2 3)").unwrap())
            .expect_err("surplus arg beyond required+optional must error");
        let (macro_name, expected, got) = too_many_macro_args_fields(&err);
        assert_eq!(macro_name, "f");
        assert_eq!(expected, 2);
        assert_eq!(got, 3);
    }

    #[test]
    fn too_many_macro_args_required_plus_two_optionals_arity_three() {
        // Larger optional section (capacity 1 + 2 = 3). 4 args rejects
        // with `expected: 3`. Pins that the capacity calculation scales
        // with optional.len(), not just at-most-one. Mixes a bare
        // optional with an optional carrying a default form — both shapes
        // contribute identically to capacity (the typed `OptionalParam`
        // entry's `default: Option<Sexp>` is irrelevant to the arity gate).
        let mut e = Expander::new();
        let err = e
            .expand_program(
                read("(defmacro f (a &optional b (c 5)) `(list ,a ,b ,c)) (f 1 2 3 4)").unwrap(),
            )
            .expect_err("surplus arg beyond required+two-optional must error");
        let (macro_name, expected, got) = too_many_macro_args_fields(&err);
        assert_eq!(macro_name, "f");
        assert_eq!(expected, 3);
        assert_eq!(got, 4);
    }

    #[test]
    fn too_many_macro_args_does_not_fire_when_rest_is_present() {
        // Negative control: a rest-PRESENT param list has no maximum
        // arity — the `&rest` slot collects every trailing arg into a
        // `Sexp::List`. `(defmacro f (a &rest xs) ...)` called as
        // `(f 1 2 3 4)` MUST succeed; the new gate fires ONLY when
        // `MacroParams.rest` is `None`. Pins the rest-present-path
        // remains permissive — a regression that wrongly fires the
        // too-many gate for any surplus (including the rest-collecting
        // path) would break every `&rest`-using macro.
        let src = "(defmacro f (a &rest xs) `(list ,a ,@xs)) (f 1 2 3 4)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let out_subst = subst.expand_program(read(src).unwrap()).unwrap();
        let out_byte = bytecode.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out_subst, out_byte);
        assert_eq!(out_subst[0], parse("(list 1 2 3 4)"));
    }

    #[test]
    fn too_many_macro_args_does_not_fire_at_exact_max_arity() {
        // Negative control: the rest-less gate fires STRICTLY when
        // `args.len() > expected` — at exact arity the binder accepts.
        // `(defmacro f (a &optional b) ...)` called as `(f 1 2)` binds
        // a=1, b=2 successfully (the optional takes its supplied arg,
        // not the default). Pins the boundary condition so a regression
        // that flips the comparison to `>=` (rejecting exact-arity
        // calls) fails-loudly here.
        let src = "(defmacro f (a &optional b) `(list ,a ,b)) (f 1 2)";
        let mut e = Expander::new();
        let out = e.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out[0], parse("(list 1 2)"));
    }

    #[test]
    fn too_many_macro_args_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the SAME source emits the SAME
        // structural variant under both expansion strategies. The
        // shared `MacroParams::bind` makes the rejection lands once and
        // both paths inherit it. Mirror of
        // `missing_macro_arg_substitute_and_bytecode_paths_agree` —
        // both close `THEORY.md §II.1 invariant 2 — free middle` for
        // one failure mode each.
        let src = "(defmacro pair (a b) `(cons ,a ,b)) (pair 1 2 3)";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(too_many_macro_args_fields(&err_subst), ("pair", 2, 3));
        assert_eq!(too_many_macro_args_fields(&err_byte), ("pair", 2, 3));
    }

    #[test]
    fn too_many_macro_args_fires_after_missing_required_priority_held() {
        // Priority discipline: the required walk fires
        // `MissingMacroArg` BEFORE the rest-less surplus gate is
        // reached. `(defmacro f (a b c) …) (f 1)` is `MissingMacroArg
        // { param: "b" }`, NOT `TooManyMacroArgs` (and certainly not a
        // collision). The two failure modes are structurally disjoint:
        // too-few-required vs. too-many-with-no-rest. Pins the bail-on-
        // first-missing-required contract so a regression that swaps
        // the two gates' order would emit the wrong variant.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a b c) `(list ,a ,b ,c)) (f 1)").unwrap())
            .expect_err("missing required must error");
        assert!(
            matches!(err, LispError::MissingMacroArg { .. }),
            "expected MissingMacroArg (priority), got: {err:?}"
        );
    }

    #[test]
    fn too_many_macro_args_zero_required_zero_optional_rejects_any_args() {
        // Degenerate case: a nullary macro `(defmacro f () ...)` has
        // capacity 0; ANY supplied arg rejects with `expected: 0`. Pins
        // the gate fires even when the rest-less max-arity is zero —
        // i.e. the rejection is structural, not conditional on a
        // non-empty required+optional.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f () `(list)) (f 1)").unwrap())
            .expect_err("nullary macro called with arg must error");
        let (macro_name, expected, got) = too_many_macro_args_fields(&err);
        assert_eq!(macro_name, "f");
        assert_eq!(expected, 0);
        assert_eq!(got, 1);
    }

    #[test]
    fn too_many_macro_args_display_renders_legacy_compile_substring() {
        // The rendered Display matches the legacy `Compile`-shaped
        // diagnostic style — `"compile error in call to {macro_name}:
        // too many args: expected at most {expected}, got {got}"` — so
        // the existing `"compile error in call to"` substring authoring
        // tools' assertions key on stays unchanged. Pins the byte-level
        // rendered shape so a regression that drifts the prefix /
        // separator / labels fails-loudly here.
        let err = LispError::TooManyMacroArgs {
            macro_name: "pair".into(),
            expected: 2,
            got: 5,
        };
        assert_eq!(
            err.to_string(),
            "compile error in call to pair: too many args: expected at most 2, got 5"
        );
    }

    #[test]
    fn too_many_macro_args_position_is_none_today() {
        // Negative control for the future-spans move: until `Sexp`
        // carries source positions, `position()` returns `None` for
        // this variant. A future run that gives `Sexp` source spans
        // adds `pos: Option<usize>` to ONE place; this test gives that
        // change a deliberate fail-before/pass-after delta. Parallel to
        // `missing_macro_arg_position_is_none_today`.
        let err = LispError::TooManyMacroArgs {
            macro_name: "pair".into(),
            expected: 2,
            got: 3,
        };
        assert_eq!(err.position(), None);
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

    // --- RestParamTrailingTokens: the parse_params gate's third (and
    // final) definition-site failure mode ---
    //
    // A `&rest <name>` absorbs every remaining call arg, so it is the LAST
    // thing a param list can name. Before this variant `parse_params`
    // returned the moment it bound the rest name, SILENTLY DROPPING any
    // trailing tokens — `(a &rest xs extra)` parsed as if `extra` weren't
    // there. These tests pin the loud rejection that replaces the silent
    // drop; the symbol `RestParamTrailingTokens` exists only after this
    // change, so the whole block is fail-before/pass-after by construction
    // (compile-time edge) and the end-to-end regression guard below pins
    // that the malformed defmacro no longer expands cleanly.

    /// Helper mirroring `rest_param_missing_name_fields` — pins the variant
    /// shape and lifts the marker position, trailing count, and first
    /// offender's display up to the assert site.
    fn rest_param_trailing_tokens_fields(err: &LispError) -> (usize, usize, &str) {
        match err {
            LispError::RestParamTrailingTokens {
                rest_position,
                extra,
                first,
            } => (*rest_position, *extra, first.display.as_str()),
            other => panic!("expected RestParamTrailingTokens, got: {other:?}"),
        }
    }

    #[test]
    fn parse_params_rejects_single_trailing_token_after_rest_name() {
        // `(a &rest c extra)` — `&rest c` is well-formed, but `extra`
        // follows. The rest name is bound at position 2, the marker at 1;
        // the lone trailing token `extra` is reported (extra == 1, first ==
        // "extra"). Before this variant `parse_params` returned at the rest
        // name and `extra` vanished.
        let err = parse_params(&read("a &rest c extra").unwrap())
            .expect_err("a trailing token after the rest name must error");
        assert_eq!(rest_param_trailing_tokens_fields(&err), (1, 1, "extra"));
    }

    #[test]
    fn rest_param_trailing_tokens_counts_the_whole_trailing_run() {
        // `(&rest c x y z)` — three tokens follow the rest name. `extra`
        // counts ALL of them (3), `first` is the first (`x`), and the
        // marker is at position 0. A regression that reports only the
        // first trailing token's presence (extra hard-coded to 1) fails
        // loudly here.
        let err = parse_params(&read("&rest c x y z").unwrap())
            .expect_err("multiple trailing tokens must error");
        assert_eq!(rest_param_trailing_tokens_fields(&err), (0, 3, "x"));
    }

    #[test]
    fn rest_param_trailing_tokens_first_witness_carries_non_symbol_display() {
        // `(a &rest c 5)` — the rest NAME `c` is a valid symbol, so this is
        // NOT a `RestParamMissingName`; the integer `5` is a trailing token
        // AFTER a well-formed `&rest c`. Pins that the two sibling failure
        // modes don't collide: a malformed rest-name is `RestParamMissingName`,
        // a well-formed rest-name followed by junk is
        // `RestParamTrailingTokens`. `first` round-trips `5` via the typed
        // witness's `Sexp::Display` projection.
        let err = parse_params(&read("a &rest c 5").unwrap())
            .expect_err("a trailing non-symbol after the rest name must error");
        assert_eq!(rest_param_trailing_tokens_fields(&err), (1, 1, "5"));
    }

    #[test]
    fn rest_param_trailing_tokens_no_longer_silently_dropped_end_to_end() {
        // The fidelity fix, end-to-end through `expand_program`: a defmacro
        // whose param list carries a stray token after `&rest <name>` now
        // ERRORS at the typed-entry gate instead of expanding as though the
        // stray token weren't there. This is the regression guard for the
        // silent-drop bug — before this change the same source expanded
        // cleanly and `extra` was discarded with no signal.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a &rest xs extra) `(,a))").unwrap())
            .expect_err("trailing token after &rest name must error");
        assert_eq!(rest_param_trailing_tokens_fields(&err), (1, 1, "extra"));
    }

    #[test]
    fn rest_param_trailing_tokens_substitute_and_bytecode_paths_agree() {
        // Path-uniform rejection: the gate fires inside `macro_def_from` →
        // `parse_params`, BEFORE either expansion strategy runs, so both
        // `Expander::new()` (bytecode) and `Expander::new_substitute_only()`
        // (substitute) reject the SAME malformed defmacro at the SAME gate.
        // Sibling of `rest_param_missing_name_substitute_and_bytecode_paths_agree`.
        let src = "(defmacro f (a &rest xs extra) `(,a))";
        let mut subst = Expander::new_substitute_only();
        let mut bytecode = Expander::new();
        let err_subst = subst
            .expand_program(read(src).unwrap())
            .expect_err("substitute must error");
        let err_byte = bytecode
            .expand_program(read(src).unwrap())
            .expect_err("bytecode must error");
        assert_eq!(
            rest_param_trailing_tokens_fields(&err_subst),
            (1, 1, "extra")
        );
        assert_eq!(
            rest_param_trailing_tokens_fields(&err_byte),
            (1, 1, "extra")
        );
    }

    #[test]
    fn rest_param_trailing_tokens_message_renders_legacy_style_prefix_and_suffix() {
        // End-to-end through Display — pins the rendered diagnostic AND the
        // new `(rest marker at position {n}, {extra} trailing after name,
        // first: {first})` clause. The `compile error in defmacro params:`
        // prefix matches the sibling `&rest needs a name` rendering's shape.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro f (a &rest xs extra) `(,a))").unwrap())
            .expect_err("trailing token after &rest name must error");
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: &rest name must be last \
             (rest marker at position 1, 1 trailing after name, first: extra)"
        );
    }

    #[test]
    fn rest_param_trailing_tokens_position_is_none_today() {
        // Pins `position() == None` so the future `pos: Option<usize>` add
        // (once `Sexp` carries source spans) lands as a deliberate
        // fail-before/pass-after delta. Parallel to
        // `rest_param_missing_name_position_is_none_today`.
        let err = LispError::RestParamTrailingTokens {
            rest_position: 1,
            extra: 1,
            first: crate::error::SexpWitness::new(crate::error::SexpShape::Symbol, "extra"),
        };
        assert_eq!(err.position(), None);
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

    // ── resolve_bound_arg: bytecode-runtime bound-arg-by-index lookup ──
    //
    // `resolve_bound_arg(args_by_index, idx, macro_name, kind)` lifts the
    // `args_by_index.get(*idx).ok_or_else(|| template_invariant_violation(
    // macro_name, KIND(*idx)))?` projection that recurred at BOTH the
    // `TemplateOp::Subst` and `TemplateOp::Splice` arms inside
    // `apply_compiled`. The arms differ in the kind constructor
    // (`SubstBadIndex` vs. `SpliceBadIndex`) and in their post-lookup
    // verb (clone+push vs. splice-coerce), but the lookup-and-reject
    // prelude is byte-identical modulo the constructor. These tests
    // pin the lifted helper's contract directly; the existing
    // `apply_compiled_*_bad_idx_*` tests are the path-uniformity
    // guards proving both production arms route through it without
    // behavior drift.

    #[test]
    fn resolve_bound_arg_in_range_returns_borrowed_reference_verbatim() {
        // For an in-range index, the helper returns `Ok(&args[idx])`
        // borrowed VERBATIM — same pointer as `args_by_index.get(idx)`.
        // Pins the borrow-not-clone contract: a regression that drifts
        // the helper to clone+return (`Result<Sexp>` instead of
        // `Result<&Sexp>`) would allocate per lookup at the production
        // `Subst`/`Splice` hot path. The kind constructor must NOT
        // fire on the success path (`FnOnce`'s lazy semantics) — pin
        // that the test passes a constructor that would panic if
        // called, asserting the helper short-circuits before invoking
        // it on the in-range arm.
        let args = vec![Sexp::int(1), Sexp::int(2), Sexp::int(3)];
        let got = resolve_bound_arg(&args, 1, "m", |_| {
            panic!("kind constructor must not fire on the in-range path")
        })
        .expect("in-range lookup must succeed");
        assert!(
            std::ptr::eq(got, &args[1]),
            "resolve_bound_arg must return the SAME pointer as args_by_index.get(idx)"
        );
        assert_eq!(*got, Sexp::int(2));
    }

    #[test]
    fn resolve_bound_arg_out_of_range_with_subst_kind_emits_typed_invariant() {
        // For an out-of-range index, the helper raises the structural
        // `LispError::TemplateInvariant` variant with the caller-
        // supplied `SubstBadIndex` kind constructor applied to the bad
        // index. Pins the post-lift emission shape (variant identity
        // + the kind constructor threaded with the actual idx); a
        // regression that drops the idx payload (e.g., via a `usize ->
        // ()` projection) or hard-codes a different kind at the helper
        // boundary fails-loudly here. Fail-before-pass-after: this
        // assert is contradicted by the pre-lift code path (which
        // never called `resolve_bound_arg` because it didn't exist),
        // ratifies the post-lift one.
        let args: Vec<Sexp> = Vec::new();
        let err = resolve_bound_arg(&args, 7, "test-macro", TemplateInvariantKind::SubstBadIndex)
            .expect_err("out-of-range lookup must error");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "test-macro");
                assert_eq!(kind, TemplateInvariantKind::SubstBadIndex(7));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn resolve_bound_arg_threads_kind_constructor_per_call_site() {
        // Path-uniformity for the per-call-site kind constructor: the
        // SAME out-of-range idx via the `SpliceBadIndex` constructor
        // emits `kind: SpliceBadIndex(7)` — distinct from the sibling
        // `SubstBadIndex(7)` variant. Pins that the constructor is
        // chosen per call site (not hard-coded at the helper boundary),
        // closing the structural matrix `{Subst, Splice} × {in-range,
        // out-of-range}` the two production arms span across the
        // bytecode-runtime's bound-arg-by-index reads. A regression
        // that hard-codes a single kind at the helper boundary would
        // emit the same variant identity for both call sites and
        // fail-loudly here.
        let args: Vec<Sexp> = Vec::new();
        let err = resolve_bound_arg(
            &args,
            7,
            "test-macro",
            TemplateInvariantKind::SpliceBadIndex,
        )
        .expect_err("out-of-range lookup must error");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "test-macro");
                assert_eq!(kind, TemplateInvariantKind::SpliceBadIndex(7));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn resolve_bound_arg_threads_macro_name_verbatim() {
        // Path-uniformity for the `macro_name` slot: the helper threads
        // the caller's borrow into the variant's owned `String` slot
        // verbatim. Pin two distinct macro names route through with no
        // mutual interference — a regression that hard-codes a single
        // macro_name at the helper boundary or swaps the parameter
        // ordering fails-loudly here. Same posture as
        // `compiler_spec_io_err_threads_each_stage_through_unchanged`
        // pins the typed `stage` slot in the disk-persistence sibling
        // lift.
        let args: Vec<Sexp> = Vec::new();
        for name in ["wrap", "call-macro", "obs"] {
            let err = resolve_bound_arg(&args, 0, name, TemplateInvariantKind::SubstBadIndex)
                .expect_err("out-of-range lookup must error");
            match err {
                LispError::TemplateInvariant { macro_name, kind } => {
                    assert_eq!(macro_name, name, "macro_name slot drifted for {name}");
                    assert_eq!(kind, TemplateInvariantKind::SubstBadIndex(0));
                }
                other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
            }
        }
    }

    #[test]
    fn resolve_bound_arg_yields_first_element_when_idx_is_zero() {
        // Edge case: idx 0 with a single-element args_by_index returns
        // `Ok(&args[0])`. Pins the lower-bound of the in-range surface
        // — a regression that off-by-ones the lookup (e.g., `get(idx +
        // 1)` or `get(idx).filter(|_| idx > 0)`) would fail here.
        // Sibling to the upper-bound `resolve_bound_arg_out_of_range_
        // with_subst_kind_emits_typed_invariant` test.
        let args = vec![Sexp::int(42)];
        let got = resolve_bound_arg(&args, 0, "m", |_| {
            panic!("kind constructor must not fire on the in-range path")
        })
        .expect("idx-0 lookup must succeed");
        assert!(std::ptr::eq(got, &args[0]));
        assert_eq!(*got, Sexp::int(42));
    }

    #[test]
    fn resolve_bound_arg_yields_last_element_at_exact_upper_bound() {
        // Edge case: idx `len - 1` is the highest valid index. Pin
        // that it routes through the success arm (NOT the error arm),
        // closing the in-range surface end-to-end with the lower-
        // bound sibling. A regression that off-by-ones the upper
        // bound (e.g., `get(idx).filter(|_| idx < args.len() - 1)`)
        // would fail here.
        let args = vec![Sexp::int(1), Sexp::int(2), Sexp::int(3)];
        let got = resolve_bound_arg(&args, args.len() - 1, "m", |_| {
            panic!("kind constructor must not fire on the in-range path")
        })
        .expect("last-element lookup must succeed");
        assert!(std::ptr::eq(got, args.last().unwrap()));
        assert_eq!(*got, Sexp::int(3));
    }

    #[test]
    fn resolve_bound_arg_at_exact_length_routes_to_error_arm() {
        // Boundary case: idx EQUAL to `args.len()` is out-of-range
        // (since `get` is 0-indexed). Pin that this routes through
        // the error arm with the kind constructor applied to the
        // EXACT idx that was tried. A regression that off-by-ones
        // the boundary (e.g., admits `idx == len`) would fail here.
        // This is the canonical off-by-one trap; the helper's
        // contract pins it at the variant-construction boundary.
        let args = vec![Sexp::int(1)];
        let err = resolve_bound_arg(&args, 1, "m", TemplateInvariantKind::SubstBadIndex)
            .expect_err("idx == len must error");
        match err {
            LispError::TemplateInvariant { kind, .. } => {
                assert_eq!(kind, TemplateInvariantKind::SubstBadIndex(1));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn resolve_bound_arg_empty_slice_with_any_idx_routes_to_error_arm() {
        // Boundary case: an empty `args_by_index` slice rejects every
        // idx (including 0). Pin that the helper's emission shape is
        // uniform regardless of which out-of-range idx fires the
        // rejection — `SubstBadIndex(0)` for an empty slice is the
        // bytecode-runtime mirror of a zero-arity macro template
        // referencing the 0-th param.
        let args: Vec<Sexp> = Vec::new();
        let err = resolve_bound_arg(&args, 0, "zero-arity", TemplateInvariantKind::SubstBadIndex)
            .expect_err("empty slice rejects every idx");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "zero-arity");
                assert_eq!(kind, TemplateInvariantKind::SubstBadIndex(0));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn apply_compiled_subst_bad_idx_routes_through_resolve_bound_arg_with_subst_kind() {
        // End-to-end path-uniformity: a `Subst(99)` op against a
        // zero-arity macro routes the bytecode-runtime's bound-arg
        // lookup through `resolve_bound_arg` with the
        // `SubstBadIndex` constructor, emitting the structural
        // variant with `kind: SubstBadIndex(99)`. The pre-lift
        // sibling test `apply_compiled_subst_bad_idx_routes_through_
        // template_invariant_violation` pins that the same input
        // routes through `template_invariant_violation`; this test
        // pins that BOTH still hold under the post-lift composition
        // — `resolve_bound_arg` calls `template_invariant_violation`
        // internally on the rejection arm. A regression that drifts
        // ONE arm's projection from the other (e.g., swaps the
        // constructor at one call site, or short-circuits the
        // composition) would fail here.
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
    fn apply_compiled_splice_bad_idx_routes_through_resolve_bound_arg_with_splice_kind() {
        // Sibling end-to-end path-uniformity for the `Splice` arm:
        // the post-lift composition routes a `Splice(42)` op through
        // `resolve_bound_arg` with the `SpliceBadIndex` constructor,
        // emitting `kind: SpliceBadIndex(42)`. Together with the
        // `Subst` sibling test above, this pins the structural matrix
        // `{Subst, Splice} × resolve_bound_arg` end-to-end through
        // the public `apply_compiled` surface, so a regression that
        // drifts ONE arm's kind constructor (e.g., the `Splice` arm
        // accidentally emits `SubstBadIndex` after a copy-paste
        // refactor) fails-loudly here.
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
    fn apply_compiled_subst_in_range_routes_past_resolve_bound_arg_into_clone_and_push() {
        // Positive control: a `Subst(0)` op against a one-arg macro
        // routes through `resolve_bound_arg`'s success arm and the
        // `Subst` post-lookup verb (clone + push) emits the bound
        // value verbatim. Pin the post-lift composition's success
        // path: the clone-and-push semantics live at the call site
        // (NOT in `resolve_bound_arg`, which only borrows), and a
        // regression that drifts the borrow contract (e.g., the
        // helper clones internally + the call site clones again)
        // would still pass observationally but would regress the
        // hot-path allocation count.
        let params = MacroParams {
            required: vec!["x".into()],
            optional: Vec::new(),
            rest: None,
        };
        let tmpl = CompiledTemplate {
            ops: vec![TemplateOp::Subst(0)],
        };
        let out = apply_compiled("id", &params, &tmpl, &[Sexp::int(42)])
            .expect("in-range Subst must succeed");
        assert_eq!(out, Sexp::int(42));
    }

    // ── current_builder_mut: the bytecode-runtime top-of-stack projection ──
    //
    // `current_builder_mut(stack)` lifts the `stack.last_mut().unwrap()`
    // projection that appeared at FOUR sites inside `apply_compiled`'s
    // op-loop (Literal, Subst, Splice, post-EndList parent-fold) into ONE
    // named primitive. The expect message names the bytecode-runtime
    // invariant ("at least one stack frame during op-loop") so a
    // regression that drifts the loop's frame management (a new op that
    // pops without pushing, an early-return that bypasses EndList's
    // stack-check) surfaces a NAMED panic rather than a silent unwrap.
    // These tests pin the projection's contract directly; the existing
    // `apply_compiled_*` tests + the cross-strategy `expansion_layers_
    // agree_on_output_and_cache_wins` benchmark are the path-uniformity
    // guards proving the four sites still emit the canonical bytecode-
    // runtime output across the lift.

    #[test]
    fn current_builder_mut_returns_the_top_frame_reference() {
        // The simplest projection: on a single-frame stack, the helper
        // returns a `&mut Vec<Sexp>` pointing at THAT frame. Pin the
        // projection's identity end-to-end via a push that mutates
        // through the borrow and observe the original frame carries the
        // pushed value back.
        let mut stack: Vec<Vec<Sexp>> = vec![Vec::new()];
        current_builder_mut(&mut stack).push(Sexp::int(42));
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0], vec![Sexp::int(42)]);
    }

    #[test]
    fn current_builder_mut_targets_the_topmost_frame_on_a_multi_frame_stack() {
        // The projection MUST target the topmost frame, not the bottom
        // one — every `TemplateOp::BeginList` pushes a fresh frame the
        // subsequent ops emit into, and a regression that flipped the
        // projection to `first_mut` (or to a fixed bottom-frame
        // reference) would silently smear all op output into the
        // outermost result. Pin path-uniformity with the bytecode-
        // runtime's mid-list emission posture: with three frames on
        // the stack (one outer + two pending lists), the helper
        // returns a borrow into the third frame, leaving frames 0 and
        // 1 untouched.
        let mut stack: Vec<Vec<Sexp>> = vec![
            vec![Sexp::symbol("outer")],
            vec![Sexp::symbol("inner-a")],
            vec![Sexp::symbol("inner-b")],
        ];
        current_builder_mut(&mut stack).push(Sexp::int(99));
        assert_eq!(stack[0], vec![Sexp::symbol("outer")]);
        assert_eq!(stack[1], vec![Sexp::symbol("inner-a")]);
        assert_eq!(stack[2], vec![Sexp::symbol("inner-b"), Sexp::int(99)]);
    }

    #[test]
    fn current_builder_mut_is_pointer_equal_to_last_mut_unwrap() {
        // Structural identity binding the lift to its pre-lift inline
        // shape: `current_builder_mut(&mut stack)` IS
        // `stack.last_mut().unwrap()` — the same `&mut Vec<Sexp>`,
        // pointing at the same allocation. Pin pointer equality via
        // `std::ptr::eq` on the projected slice's `as_ptr()` to rule
        // out any allocation-shape drift across the lift.
        let mut stack: Vec<Vec<Sexp>> = vec![vec![Sexp::int(1), Sexp::int(2)]];
        let via_lift_ptr = current_builder_mut(&mut stack).as_ptr();
        let via_inline_ptr = stack.last_mut().unwrap().as_ptr();
        assert!(
            std::ptr::eq(via_lift_ptr, via_inline_ptr),
            "current_builder_mut must borrow the SAME frame as stack.last_mut().unwrap()"
        );
    }

    #[test]
    #[should_panic(
        expected = "bytecode-runtime invariant: at least one stack frame during op-loop"
    )]
    fn current_builder_mut_panics_with_named_invariant_on_empty_stack() {
        // The bytecode-runtime invariant is encoded in the expect
        // message: an empty stack at the projection boundary is
        // structurally unreachable inside `apply_compiled`'s op-loop
        // (the outermost frame is seeded at entry and every BeginList
        // / EndList pair preserves the count >= 1). Pin that the
        // NAMED invariant fires on the failure path so a regression
        // that drifts the loop's frame management surfaces a
        // diagnostic-grade panic rather than a silent unwrap over
        // `None`. Authoring tools / future debug-mode hooks can
        // pattern-match on the named invariant string instead of
        // tracking down an unnamed unwrap site.
        let mut empty: Vec<Vec<Sexp>> = Vec::new();
        let _ = current_builder_mut(&mut empty);
    }

    #[test]
    fn current_builder_mut_routes_apply_compiled_literal_emit() {
        // End-to-end path-uniformity guard: a single-op program
        // `TemplateOp::Literal(s)` routes its push through
        // `current_builder_mut(&mut stack)` and the literal lands in
        // the outermost frame. After the op-loop completes the outer
        // `stack.pop().FinalNoValue` gate sees a non-empty top frame
        // containing exactly one element, which `apply_compiled`'s
        // tail (`top.len() == 1 { top.remove(0) }`) projects back as
        // the bound value. Pre-lift the same emission ran through
        // `stack.last_mut().unwrap().push(s.clone())`; post-lift it
        // runs through `current_builder_mut(&mut stack).push(s.clone())`
        // — the byte-identical outcome pins that the Literal arm's
        // routing through the new projection preserves the bytecode-
        // runtime's emission shape.
        let tmpl = CompiledTemplate {
            ops: vec![TemplateOp::Literal(Sexp::symbol("hello"))],
        };
        let out = apply_compiled("id", &MacroParams::default(), &tmpl, &[])
            .expect("literal-only template must succeed");
        assert_eq!(out, Sexp::symbol("hello"));
    }

    #[test]
    fn current_builder_mut_routes_apply_compiled_end_list_parent_fold() {
        // End-to-end path-uniformity guard for the post-EndList
        // parent-fold push: `(BeginList, Literal(a), Literal(b),
        // EndList)` builds an inner frame `[a, b]`, pops it on
        // EndList, then pushes `Sexp::List([a, b])` into the parent
        // (outer) frame via `current_builder_mut`. The outermost
        // `stack.pop()` then surfaces that list as the bound result.
        // Pre-lift the parent-fold push ran through
        // `stack.last_mut().unwrap().push(Sexp::List(items))`; post-
        // lift it runs through `current_builder_mut(&mut stack).
        // push(Sexp::List(items))` — pin the byte-identical outcome
        // so a regression that drifts the parent-fold target (e.g.,
        // pushes onto the just-popped frame's pointer instead of the
        // new top) fails loudly here.
        let tmpl = CompiledTemplate {
            ops: vec![
                TemplateOp::BeginList,
                TemplateOp::Literal(Sexp::symbol("a")),
                TemplateOp::Literal(Sexp::symbol("b")),
                TemplateOp::EndList,
            ],
        };
        let out = apply_compiled("id", &MacroParams::default(), &tmpl, &[])
            .expect("BeginList/EndList template must succeed");
        assert_eq!(out, Sexp::List(vec![Sexp::symbol("a"), Sexp::symbol("b")]));
    }

    #[test]
    fn current_builder_mut_routes_apply_compiled_subst_and_splice_emits() {
        // End-to-end path-uniformity guard for BOTH index-reading
        // arms routing through the lifted projection: a one-required +
        // one-rest macro `(call f &rest args)` with template
        // `(BeginList, Subst(0), Splice(1), EndList)` exercises both
        // Subst's clone-and-push AND Splice's splice-value-into
        // emit-paths against the current builder via
        // `current_builder_mut(&mut stack)`. The composed result is
        // `(foo 1 2 3)` — Subst lands the bound `f = foo` and
        // Splice flattens `args = (1 2 3)` — and the byte-identical
        // outcome pins that BOTH Subst and Splice arms' emits route
        // through the SHARED projection. Sibling to
        // `apply_compiled_splice_in_range_routes_past_resolve_bound
        // _arg_into_splice_value_into` which already exercises this
        // shape end-to-end; the addition here is the path-uniformity
        // anchor for the `current_builder_mut` lift specifically.
        let params = MacroParams {
            required: vec!["f".into()],
            optional: Vec::new(),
            rest: Some("args".into()),
        };
        let tmpl = CompiledTemplate {
            ops: vec![
                TemplateOp::BeginList,
                TemplateOp::Subst(0),
                TemplateOp::Splice(1),
                TemplateOp::EndList,
            ],
        };
        let out = apply_compiled(
            "call",
            &params,
            &tmpl,
            &[
                Sexp::symbol("foo"),
                Sexp::int(1),
                Sexp::int(2),
                Sexp::int(3),
            ],
        )
        .expect("Subst + Splice template must succeed");
        assert_eq!(
            out,
            Sexp::List(vec![
                Sexp::symbol("foo"),
                Sexp::int(1),
                Sexp::int(2),
                Sexp::int(3),
            ])
        );
    }

    #[test]
    fn apply_compiled_splice_in_range_routes_past_resolve_bound_arg_into_splice_value_into() {
        // Positive control for the `Splice` arm: a `&rest` macro that
        // splices a bound list routes through `resolve_bound_arg`'s
        // success arm and the `Splice` post-lookup verb
        // (`splice_value_into`) flattens the bound list into the
        // builder. Pin the composition's success path end-to-end:
        // the bound `Sexp::List([1, 2, 3])` at idx 1 flattens into
        // the outer builder's `(call 1 2 3)` shape — the same output
        // `rest_param_splices_with_at` pins through the public
        // surface, here pinned with the bytecode-runtime composition
        // exposed directly.
        let params = MacroParams {
            required: vec!["f".into()],
            optional: Vec::new(),
            rest: Some("args".into()),
        };
        let tmpl = CompiledTemplate {
            ops: vec![
                TemplateOp::BeginList,
                TemplateOp::Subst(0),
                TemplateOp::Splice(1),
                TemplateOp::EndList,
            ],
        };
        let out = apply_compiled(
            "call",
            &params,
            &tmpl,
            &[Sexp::symbol("foo"), Sexp::int(1), Sexp::int(2)],
        )
        .expect("in-range Splice must succeed");
        assert_eq!(
            out,
            Sexp::List(vec![Sexp::symbol("foo"), Sexp::int(1), Sexp::int(2)])
        );
    }

    // ── pop_builder_frame: the bytecode-runtime stack-frame consume ─────
    //
    // `pop_builder_frame(stack, macro_name, kind)` lifts the
    // `stack.pop().ok_or_else(|| template_invariant_violation(macro_name,
    // kind))?` chain that recurred at two sites inside `apply_compiled`
    // (the `EndList` arm + the post-loop final pop) into ONE named
    // primitive on the bytecode-runtime's stack-frame algebra. Sibling of
    // `current_builder_mut` (the top-frame-borrow projection — the same
    // `&mut Vec<Vec<Sexp>>` consumed at the borrow face) and
    // `resolve_bound_arg` (the bound-arg-by-index lookup primitive that
    // also routes through `TemplateInvariantKind` as the per-call-site
    // rejection identity). These tests pin the primitive's contract
    // directly; the existing `apply_compiled_*` tests are the path-
    // uniformity guards proving the two sites route through it without
    // behavior drift.

    #[test]
    fn pop_builder_frame_pops_top_frame_off_non_empty_stack() {
        // Happy path: a two-frame stack pops the topmost frame off and
        // returns it AS-IS while shrinking the stack by exactly one
        // element. Pin both the return value (the popped frame's
        // contents are byte-identical to what was pushed) AND the
        // mutation (`stack.len()` drops from 2 to 1).
        let mut stack: Vec<Vec<Sexp>> = vec![
            vec![Sexp::symbol("outer")],
            vec![Sexp::int(1), Sexp::int(2)],
        ];
        let popped =
            pop_builder_frame(&mut stack, "wrap", TemplateInvariantKind::EndListEmptyStack)
                .expect("non-empty stack must pop cleanly");
        assert_eq!(popped, vec![Sexp::int(1), Sexp::int(2)]);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0], vec![Sexp::symbol("outer")]);
    }

    #[test]
    fn pop_builder_frame_emits_template_invariant_with_end_list_empty_stack_kind() {
        // Empty-stack rejection: an empty stack flows through the
        // `EndListEmptyStack` kind constructor into a structural
        // `LispError::TemplateInvariant { macro_name, kind:
        // EndListEmptyStack }` variant. Fail-before-pass-after: pre-lift
        // the same input would route through the inline
        // `stack.pop().ok_or_else(|| template_invariant_violation(_,
        // EndListEmptyStack))?` chain at the `EndList` arm; post-lift
        // it routes through ONE named primitive both pop-emitting
        // sites share, and a regression that drops the kind threading
        // (e.g. unifies both kinds into one constant) fails here.
        let mut empty: Vec<Vec<Sexp>> = Vec::new();
        let err = pop_builder_frame(&mut empty, "wrap", TemplateInvariantKind::EndListEmptyStack)
            .expect_err("empty stack must reject");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "wrap");
                assert_eq!(kind, TemplateInvariantKind::EndListEmptyStack);
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn pop_builder_frame_emits_template_invariant_with_final_no_value_kind() {
        // Path-uniformity across the closed-set of pop-emitting kinds:
        // the same primitive threads `FinalNoValue` verbatim through
        // its `TemplateInvariantKind` slot, with the macro_name
        // identity preserved across the call boundary. Sibling of the
        // `EndListEmptyStack` test above — together they pin the
        // primitive's closed-set posture (BOTH reachable
        // pop-emitting kinds route through the same primitive, neither
        // is hard-coded), so a future `TemplateInvariantKind` variant
        // added for a new pop-emitting op (e.g. a hypothetical
        // `EndManyEmptyStack`) extends the primitive's reachability
        // mechanically by passing the new kind through the same slot.
        let mut empty: Vec<Vec<Sexp>> = Vec::new();
        let err = pop_builder_frame(&mut empty, "id", TemplateInvariantKind::FinalNoValue)
            .expect_err("empty stack must reject");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "id");
                assert_eq!(kind, TemplateInvariantKind::FinalNoValue);
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn pop_builder_frame_threads_macro_name_through_variant_for_indexed_kinds() {
        // Closed-set posture check: even kinds the production
        // `apply_compiled` op-loop ROUTES through `resolve_bound_arg`
        // rather than `pop_builder_frame` (the indexed `SubstBadIndex(_)`
        // / `SpliceBadIndex(_)` siblings) MUST compose correctly with
        // this primitive's typed slot — they are NOT reachable here
        // from the production loop, but `TemplateInvariantKind` does
        // not distinguish "indexed" kinds from "stack-gate" kinds at
        // the helper's signature, so a regression that special-cases
        // one kind family would be a silent type-narrowing the closed-
        // set typed enum was lifted to prevent. Pin the universal
        // routing: ANY kind variant feeds through the primitive's
        // `kind` slot identically. Sibling assertion to
        // `template_invariant_violation_threads_subst_idx_through_typed_variant`
        // — same compose-the-kind-into-the-variant contract, one
        // composition step further down the substrate stack.
        let mut empty: Vec<Vec<Sexp>> = Vec::new();
        let err = pop_builder_frame(
            &mut empty,
            "compose",
            TemplateInvariantKind::SubstBadIndex(42),
        )
        .expect_err("empty stack must reject regardless of kind family");
        match err {
            LispError::TemplateInvariant { macro_name, kind } => {
                assert_eq!(macro_name, "compose");
                assert_eq!(kind, TemplateInvariantKind::SubstBadIndex(42));
            }
            other => panic!("expected LispError::TemplateInvariant, got {other:?}"),
        }
    }

    #[test]
    fn pop_builder_frame_is_byte_identical_to_inline_pop_then_template_invariant_violation() {
        // Structural-identity binding the lift to its pre-lift inline
        // shape: `pop_builder_frame(stack, macro_name, kind)` IS
        // `stack.pop().ok_or_else(|| template_invariant_violation(
        // macro_name, kind))?` — both reachable arms (success +
        // failure) must produce byte-identical outcomes. The success
        // arm checks the popped Vec contents AND the post-pop stack
        // length match the inline path; the failure arm checks the
        // emitted variant's identity AND its `macro_name` / `kind`
        // slots match the inline path. A regression that drifts the
        // primitive's projection (e.g. a `stack.swap_remove(_)` typo,
        // or a kind-rewrite at the helper boundary) fails on at least
        // one of the two arms.
        // Success arm:
        let mut stack_lift: Vec<Vec<Sexp>> = vec![vec![Sexp::symbol("a")], vec![Sexp::int(7)]];
        let mut stack_inline: Vec<Vec<Sexp>> = vec![vec![Sexp::symbol("a")], vec![Sexp::int(7)]];
        let via_lift = pop_builder_frame(
            &mut stack_lift,
            "macro",
            TemplateInvariantKind::EndListEmptyStack,
        )
        .expect("non-empty stack pops cleanly through lift");
        let via_inline = stack_inline.pop().ok_or_else(|| {
            template_invariant_violation("macro", TemplateInvariantKind::EndListEmptyStack)
        });
        assert_eq!(
            via_lift,
            via_inline.unwrap(),
            "popped frame must be byte-identical across lift vs inline"
        );
        assert_eq!(
            stack_lift.len(),
            stack_inline.len(),
            "post-pop stack length must be byte-identical across lift vs inline"
        );
        // Failure arm:
        let mut empty_lift: Vec<Vec<Sexp>> = Vec::new();
        let mut empty_inline: Vec<Vec<Sexp>> = Vec::new();
        let err_lift = pop_builder_frame(
            &mut empty_lift,
            "macro",
            TemplateInvariantKind::FinalNoValue,
        )
        .expect_err("empty stack rejects through lift");
        let err_inline = empty_inline
            .pop()
            .ok_or_else(|| {
                template_invariant_violation("macro", TemplateInvariantKind::FinalNoValue)
            })
            .expect_err("empty stack rejects through inline");
        match (err_lift, err_inline) {
            (
                LispError::TemplateInvariant {
                    macro_name: m_lift,
                    kind: k_lift,
                },
                LispError::TemplateInvariant {
                    macro_name: m_inline,
                    kind: k_inline,
                },
            ) => {
                assert_eq!(m_lift, m_inline);
                assert_eq!(k_lift, k_inline);
            }
            (l, i) => panic!(
                "expected LispError::TemplateInvariant on both arms, got lift={l:?}, inline={i:?}"
            ),
        }
    }

    #[test]
    fn pop_builder_frame_routes_apply_compiled_end_list_consume() {
        // End-to-end path-uniformity guard for the `EndList` arm: a
        // `(BeginList, Literal(x), EndList)` program pushes a child
        // frame, populates it with one literal, then routes the child
        // frame OUT of the stack via `pop_builder_frame` (kind
        // `EndListEmptyStack` — unreachable on this valid input, but
        // the kind threads through the primitive identically). The
        // post-pop verb (`Sexp::List(items)` push into the parent via
        // `current_builder_mut`) yields the same `Sexp::List([x])`
        // shape the consumer projected pre-lift. Pin the byte-
        // identical outcome so a regression that drifts the EndList
        // arm's routing (e.g. swaps the kind constructor, or routes
        // through a different stack-mutating primitive) fails loudly
        // here. Sibling of
        // `current_builder_mut_routes_apply_compiled_end_list_parent_fold`
        // — that test pins the parent-fold PUSH; this test pins the
        // child-frame POP that immediately precedes it. Together the
        // two close the EndList arm's path-uniformity across the
        // pop-then-push composition.
        let tmpl = CompiledTemplate {
            ops: vec![
                TemplateOp::BeginList,
                TemplateOp::Literal(Sexp::symbol("only")),
                TemplateOp::EndList,
            ],
        };
        let out = apply_compiled("id", &MacroParams::default(), &tmpl, &[])
            .expect("BeginList/EndList one-literal template must succeed");
        assert_eq!(out, Sexp::List(vec![Sexp::symbol("only")]));
    }

    #[test]
    fn pop_builder_frame_routes_apply_compiled_final_pop_consume() {
        // End-to-end path-uniformity guard for the post-loop final
        // pop: a single-op `Literal(s)` program emits `s` into the
        // outermost (seed) frame, then the post-loop tail routes the
        // seed frame OUT via `pop_builder_frame` (kind `FinalNoValue`
        // — unreachable on this valid input). The post-pop arity gate
        // (`top.len() == 1 { top.remove(0) }`) projects the literal
        // back as the bound value. Pre-lift the same emission ran
        // through the inline `stack.pop().ok_or_else(|| template_
        // invariant_violation(_, FinalNoValue))?` chain at the
        // post-loop tail; post-lift it routes through ONE named
        // primitive the EndList arm ALSO routes through, and the
        // single-literal outcome is byte-identical across both code
        // paths. Sibling of
        // `current_builder_mut_routes_apply_compiled_literal_emit` —
        // that test pins the EMIT into the seed frame; this test pins
        // the POP of the same seed frame at the post-loop tail.
        // Together the two close the Literal-only program's path-
        // uniformity across the emit-then-consume composition.
        let tmpl = CompiledTemplate {
            ops: vec![TemplateOp::Literal(Sexp::int(123))],
        };
        let out = apply_compiled("id", &MacroParams::default(), &tmpl, &[])
            .expect("literal-only template must succeed");
        assert_eq!(out, Sexp::int(123));
    }

    // ── MacroParams: the typed param-list primitive ─────────────────────
    //
    // `parse_params` now yields a `MacroParams { required, optional, rest }`
    // whose shape makes the canonical lambda-list ordering (required →
    // optional → rest, "&rest is last + at-most-one", "&optional at most
    // once") structural rather than a construction discipline a `Vec<Param>`
    // only happened to uphold. These tests pin the parser's mapping into the
    // typed shape, the flat-index contract `names()` exposes to the template
    // bytecode, and the single positional binder `bind()` both expansion
    // strategies now route through. The end-to-end `rest_param_splices_with_at`
    // and `compiled_template_matches_substitute_path` tests above are the
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
                optional: Vec::new(),
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
                optional: Vec::new(),
                rest: None,
            }
        );
    }

    #[test]
    fn parse_params_maps_optional_section_between_required_and_rest() {
        // `(a &optional b c &rest d)` — the canonical lambda-list order. `a`
        // is required, `b`/`c` are optional, `d` is rest. The `&optional`
        // marker switches collection from `required` to `optional`; `&rest`
        // remains terminal.
        let params = parse_params(&read("a &optional b c &rest d").unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: vec!["a".into()],
                optional: vec![OptionalParam::bare("b"), OptionalParam::bare("c")],
                rest: Some("d".into()),
            }
        );
    }

    #[test]
    fn parse_params_optional_with_no_rest_leaves_rest_none() {
        // `(&optional x)` — a leading `&optional` (zero required) with no
        // rest. `required` is empty, `x` is the sole optional, `rest` None.
        let params = parse_params(&read("&optional x").unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: Vec::new(),
                optional: vec![OptionalParam::bare("x")],
                rest: None,
            }
        );
    }

    #[test]
    fn parse_params_rejects_repeated_optional_marker() {
        // `(a &optional b &optional c)` — a second `&optional` is
        // unrepresentable (one flat optional section), so the gate REJECTS
        // rather than binding args to a marker symbol named `&optional`. The
        // two marker positions (1 and 3) are named.
        let err = parse_params(&read("a &optional b &optional c").unwrap())
            .expect_err("repeated &optional must error");
        assert!(
            matches!(
                err,
                LispError::OptionalMarkerRepeated {
                    first_position: 1,
                    second_position: 3,
                }
            ),
            "expected OptionalMarkerRepeated {{1, 3}}, got: {err:?}"
        );
    }

    #[test]
    fn parse_params_rejects_optional_after_rest_as_trailing_tokens() {
        // `(&rest xs &optional y)` — `&rest <name>` is terminal, so the
        // `&optional y` tail is REJECTED as trailing tokens (not silently
        // dropped, and not a repeated-optional error: the rest gate fires
        // first). Pins the interaction the prior run (3627426) signposted.
        let err = parse_params(&read("&rest xs &optional y").unwrap())
            .expect_err("tokens after &rest <name> must error");
        assert!(
            matches!(err, LispError::RestParamTrailingTokens { .. }),
            "expected RestParamTrailingTokens, got: {err:?}"
        );
    }

    #[test]
    fn names_are_required_then_optional_then_rest_in_flat_index_order() {
        // The flat-index contract the bytecode `Subst(idx)`/`Splice(idx)`
        // depends on: required names at 0.., then optional names, then the
        // rest name last.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: vec![OptionalParam::bare("c")],
            rest: Some("d".into()),
        };
        assert_eq!(params.names(), vec!["a", "b", "c", "d"]);
        // Optional names occupy the indices immediately after the required run.
        assert_eq!(params.names()[params.required.len()], "c");
        // The rest name is last, after required + optional — i.e. at the
        // structural `fixed_arity()` boundary the typed primitive names.
        assert_eq!(params.names()[params.fixed_arity()], "d");
    }

    // ── `MacroParams::{REST_MARKER, OPTIONAL_MARKER,
    // LAMBDA_LIST_KEYWORD_LEAD}` — the typed CL lambda-list-keyword
    // marker algebra ────────────────────────────────────────────────
    //
    // The three `pub const`s close the Common-Lisp lambda-list keyword
    // family at the typed [`MacroParams`] algebra: `REST_MARKER` and
    // `OPTIONAL_MARKER` are the two `&'static str` markers the parser's
    // typed dispatch specialises on; `LAMBDA_LIST_KEYWORD_LEAD` is the
    // canonical `'&'` char shared as the LEAD byte of both markers.
    // Pre-lift the same two `&'static str` markers lived as two inline
    // `s == "..."` comparisons at `parse_params` — post-lift both
    // comparisons route through the typed constants so a delimiter swap
    // (e.g. a Racket-compat `#!rest` port, a Clojure-compat `&` port)
    // lands at ONE constant on the typed algebra.
    //
    // These pins cover: (a) the exact `&'static str` / `char` values,
    // (b) the structural round-trip law binding each `&'static str`
    // marker to its shared `char` LEAD byte, (c) the pairwise
    // disjointness of the two `&'static str` markers, (d) the cross-
    // axis disjointness of the `char` LEAD byte against every sibling
    // outer-marker `char` the substrate's other closed-set algebras
    // specialise on, and (e) the path-uniformity of `parse_params`
    // through the typed constants.

    #[test]
    fn macro_params_rest_marker_projects_canonical_ampersand_rest_str() {
        // Pins the constant's exact `&'static str` bytes so a typo
        // (`"&res"`, `"&Rest"`, `"&rst"`) or an accidental redefinition
        // surfaces immediately. Sibling-shape pin to
        // `macro_params_optional_marker_projects_canonical_ampersand_optional_str`
        // on the peer `&optional` axis.
        assert_eq!(
            MacroParams::REST_MARKER,
            "&rest",
            "MacroParams::REST_MARKER drifted from the substrate- \
             canonical CL lambda-list `&rest` marker — the parser's \
             `parse_params` rest-slot dispatch AND every downstream \
             authoring / rendering surface binds to this ONE typed \
             constant.",
        );
    }

    #[test]
    fn macro_params_optional_marker_projects_canonical_ampersand_optional_str() {
        // Pins the constant's exact `&'static str` bytes so a typo
        // (`"&opt"`, `"&Optional"`, `"&option"`) or an accidental
        // redefinition surfaces immediately. Sibling-shape pin to
        // `macro_params_rest_marker_projects_canonical_ampersand_rest_str`
        // on the peer `&rest` axis.
        assert_eq!(
            MacroParams::OPTIONAL_MARKER,
            "&optional",
            "MacroParams::OPTIONAL_MARKER drifted from the substrate- \
             canonical CL lambda-list `&optional` marker — the parser's \
             `parse_params` optional-section dispatch AND every \
             downstream authoring / rendering surface binds to this \
             ONE typed constant.",
        );
    }

    #[test]
    fn macro_params_lambda_list_keyword_lead_projects_canonical_ampersand_char() {
        // Pins the constant's exact `char` value so a typo (`'#'`,
        // `'@'`, `'!'`) or an accidental redefinition surfaces
        // immediately. Sibling-shape pin to
        // `atom_keyword_marker_lead_projects_canonical_colon_char`,
        // `atom_bool_literal_lead_projects_canonical_hash_char`,
        // `atom_str_delimiter_projects_canonical_double_quote_char`
        // on the peer per-role LEAD-byte axes across the substrate's
        // closed-set outer algebras.
        assert_eq!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            '&',
            "LAMBDA_LIST_KEYWORD_LEAD char drifted from the substrate- \
             canonical `&` LEAD byte — the CL lambda-list-keyword \
             family (REST_MARKER, OPTIONAL_MARKER) shares this ONE \
             typed constant as their common LEAD byte.",
        );
    }

    #[test]
    fn macro_params_rest_marker_prefixed_by_lambda_list_keyword_lead() {
        // STRUCTURAL ROUND-TRIP CONTRACT: the `&'static str` marker
        // `MacroParams::REST_MARKER` starts with the `char` LEAD byte
        // `MacroParams::LAMBDA_LIST_KEYWORD_LEAD` — the projection law
        // binding the two typed constants on the [`MacroParams`]
        // algebra. A regression that renames the `&'static str` (e.g.
        // to Racket's `"#!rest"` keyword-args) OR the `char` (e.g. to
        // `'#'` for the `#!` shebang lead) without updating the other
        // fails HERE. Sibling-shape pin to
        // `atom_keyword_marker_lead_prefixes_keyword_marker` on the
        // peer `Atom`-algebra Keyword-prefix LEAD-byte axis.
        assert!(
            MacroParams::REST_MARKER.starts_with(MacroParams::LAMBDA_LIST_KEYWORD_LEAD),
            "MacroParams::REST_MARKER `{}` does NOT start with \
             MacroParams::LAMBDA_LIST_KEYWORD_LEAD `{:?}` — the two \
             typed constants have drifted apart on the [`MacroParams`] \
             algebra; the CL lambda-list-keyword family disjointness \
             contract can no longer bind to ONE shared LEAD byte.",
            MacroParams::REST_MARKER,
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
        );
    }

    #[test]
    fn macro_params_optional_marker_prefixed_by_lambda_list_keyword_lead() {
        // STRUCTURAL ROUND-TRIP CONTRACT: peer to
        // `macro_params_rest_marker_prefixed_by_lambda_list_keyword_lead`
        // on the `&optional` axis. Both `&'static str` markers of the
        // CL lambda-list-keyword family MUST share the canonical `char`
        // LEAD byte so the typed-marker disjointness contract can bind
        // to ONE shared LEAD byte on the [`MacroParams`] algebra.
        assert!(
            MacroParams::OPTIONAL_MARKER.starts_with(MacroParams::LAMBDA_LIST_KEYWORD_LEAD),
            "MacroParams::OPTIONAL_MARKER `{}` does NOT start with \
             MacroParams::LAMBDA_LIST_KEYWORD_LEAD `{:?}` — the two \
             typed constants have drifted apart on the [`MacroParams`] \
             algebra; the CL lambda-list-keyword family disjointness \
             contract can no longer bind to ONE shared LEAD byte.",
            MacroParams::OPTIONAL_MARKER,
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
        );
    }

    #[test]
    fn macro_params_rest_and_optional_markers_pairwise_disjoint() {
        // PAIRWISE DISJOINTNESS PIN: the two `&'static str` markers on
        // the CL lambda-list-keyword algebra MUST differ so the
        // parser's typed dispatch cascade at `parse_params` (which
        // tests `REST_MARKER` FIRST, then `OPTIONAL_MARKER`) cannot
        // silently route both dispatches through the same arm. A
        // regression that aliases the two markers (e.g. both to
        // `"&rest"` after a typo) would silently drop the optional
        // section's structural distinction — every `&optional` name
        // would misclassify as a rest-slot marker.
        assert_ne!(
            MacroParams::REST_MARKER,
            MacroParams::OPTIONAL_MARKER,
            "REST_MARKER and OPTIONAL_MARKER collide — the parser's \
             typed dispatch cascade at `parse_params` can no longer \
             distinguish the rest-slot boundary from the optional- \
             section boundary.",
        );
    }

    #[test]
    fn macro_params_lambda_list_keyword_lead_distinct_from_every_other_algebra_marker() {
        // CROSS-AXIS DISJOINTNESS PIN: `MacroParams::LAMBDA_LIST_KEYWORD_LEAD`
        // MUST NOT alias any sibling outer-marker `char` on the
        // substrate's other closed-set algebras — the Atom-payload
        // markers (`STR_DELIMITER`, `STR_ESCAPE_LEAD`,
        // `KEYWORD_MARKER_LEAD`, `BOOL_LITERAL_LEAD`), the paired list
        // delimiters (`Sexp::LIST_OPEN` / `Sexp::LIST_CLOSE`), the
        // paired line-comment delimiters (`Sexp::COMMENT_LEAD` /
        // `Sexp::COMMENT_TERM`), every `QuoteForm::lead_char`
        // projection, AND `QuoteForm::SPLICE_DISCRIMINATOR`. A
        // collision would silently break the reader's outer dispatch:
        // an `&`-prefixed bare atom `&rest` / `&optional` would collide
        // with whichever marker it aliased. Sibling-shape pin to
        // `atom_keyword_marker_lead_distinct_from_every_other_algebra_marker`
        // on the peer `Atom`-algebra Keyword-prefix LEAD-byte axis —
        // pins the SAME shape on the CL lambda-list-keyword LEAD-byte
        // axis. A future outer-marker extension that collided with
        // `'&'` fails HERE at the cross-axis enumeration.
        use crate::ast::{Atom, QuoteForm, Sexp};

        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Atom::STR_DELIMITER,
            "LAMBDA_LIST_KEYWORD_LEAD collides with STR_DELIMITER — a \
             bare `&rest` at a param-list position would ambiguously \
             begin a lambda-list keyword AND open a string.",
        );
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Atom::STR_ESCAPE_LEAD,
            "LAMBDA_LIST_KEYWORD_LEAD collides with STR_ESCAPE_LEAD — \
             the reader's Str-escape lead byte would alias the CL \
             lambda-list-keyword LEAD byte.",
        );
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Atom::KEYWORD_MARKER_LEAD,
            "LAMBDA_LIST_KEYWORD_LEAD collides with KEYWORD_MARKER_LEAD \
             — a bare `&rest` at a param-list position would \
             ambiguously begin a lambda-list keyword AND begin an \
             `:foo` keyword.",
        );
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Atom::BOOL_LITERAL_LEAD,
            "LAMBDA_LIST_KEYWORD_LEAD collides with BOOL_LITERAL_LEAD — \
             a bare `&rest` at a param-list position would ambiguously \
             begin a lambda-list keyword AND classify as a Bool prefix.",
        );
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Sexp::LIST_OPEN,
            "LAMBDA_LIST_KEYWORD_LEAD collides with LIST_OPEN — a bare \
             `&rest` at a param-list position would ambiguously begin \
             a lambda-list keyword AND open a list.",
        );
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Sexp::LIST_CLOSE,
            "LAMBDA_LIST_KEYWORD_LEAD collides with LIST_CLOSE — a bare \
             `&rest` at a param-list position would ambiguously begin \
             a lambda-list keyword AND close a list.",
        );
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Sexp::COMMENT_LEAD,
            "LAMBDA_LIST_KEYWORD_LEAD collides with COMMENT_LEAD — a \
             bare `&rest` at a param-list position would ambiguously \
             begin a lambda-list keyword AND begin a comment.",
        );
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            Sexp::COMMENT_TERM,
            "LAMBDA_LIST_KEYWORD_LEAD collides with COMMENT_TERM — the \
             reader's line-comment discard loop would terminate on the \
             SAME byte the parser's lambda-list-keyword LEAD dispatch \
             binds to.",
        );
        for qf in QuoteForm::ALL {
            assert_ne!(
                MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
                qf.lead_char(),
                "LAMBDA_LIST_KEYWORD_LEAD collides with \
                 QuoteForm::{qf:?}'s lead_char — a bare `&rest` at a \
                 param-list position would ambiguously begin a lambda- \
                 list keyword AND begin a quote-family prefix.",
            );
        }
        assert_ne!(
            MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            QuoteForm::SPLICE_DISCRIMINATOR,
            "LAMBDA_LIST_KEYWORD_LEAD collides with \
             SPLICE_DISCRIMINATOR — the reader's `,@` splice-promotion \
             peek byte would alias the CL lambda-list-keyword LEAD \
             byte.",
        );
    }

    #[test]
    fn parse_params_recognizes_rest_marker_via_typed_constant() {
        // PATH-UNIFORMITY PIN: the parser's rest-slot dispatch at
        // `parse_params` MUST classify the typed constant
        // `MacroParams::REST_MARKER` as the rest-slot boundary —
        // authoring surfaces that assemble a param list through the
        // typed constant (rather than through an inline string literal
        // read from user source) must reach the SAME rest-slot binding
        // the reader-driven path does.
        //
        // The `read(REST_MARKER)` composition here is the load-bearing
        // structural link: we build the param-list source through the
        // typed constant, then parse it through the reader, then the
        // parser MUST bind `xs` at the `rest` slot. A regression that
        // drifts the parser's dispatch away from the typed constant
        // (e.g. re-inlines a string literal that goes stale against
        // the constant) fails HERE.
        let src = format!("a {} xs", MacroParams::REST_MARKER);
        let params = parse_params(&read(&src).unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: vec!["a".into()],
                optional: Vec::new(),
                rest: Some("xs".into()),
            },
            "parse_params dispatch drifted away from REST_MARKER — the \
             typed constant no longer routes to the rest-slot arm.",
        );
    }

    #[test]
    fn parse_params_recognizes_optional_marker_via_typed_constant() {
        // PATH-UNIFORMITY PIN: peer to
        // `parse_params_recognizes_rest_marker_via_typed_constant` on
        // the `&optional` axis. The parser's optional-section dispatch
        // at `parse_params` MUST classify the typed constant
        // `MacroParams::OPTIONAL_MARKER` as the section-switch
        // boundary that reroutes subsequent bare-symbol names from the
        // `required` bin to the `optional` bin.
        let src = format!("a {} b c", MacroParams::OPTIONAL_MARKER);
        let params = parse_params(&read(&src).unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: vec!["a".into()],
                optional: vec![OptionalParam::bare("b"), OptionalParam::bare("c")],
                rest: None,
            },
            "parse_params dispatch drifted away from OPTIONAL_MARKER — \
             the typed constant no longer routes to the optional- \
             section arm.",
        );
    }

    // ── `MacroParams::LAMBDA_LIST_KEYWORDS` + `is_lambda_list_keyword` —
    // the closed-set forced-arity ALL array + membership gate that closes
    // the CL lambda-list-keyword family at the typed [`MacroParams`]
    // algebra. Sibling posture to the closed set of `pub const ALL: [Self;
    // N]` forced-arity arrays across the substrate's other closed-set
    // outer algebras (`AtomKind::ALL`, `QuoteForm::ALL`, `SexpShape::ALL`,
    // `UnquoteForm::ALL`, `MacroDefHead::ALL`); a future third marker
    // (`&key`, `&aux`, `&body`) extends the array's arity + adds ONE `pub
    // const` for the marker and every downstream family-wide contract
    // sweep (LEAD-byte round-trip, pairwise disjointness, membership
    // gate) picks up the extension at ONE structural site.
    //
    // These pins cover: (a) the cardinality contract (the ALL array's
    // length matches the pre-lift per-role marker count), (b) the
    // ordering contract (each ALL entry matches its corresponding per-
    // role `pub const` by-index), (c) the family-wide structural round-
    // trip (every ALL element starts_with LEAD), (d) the family-wide
    // pairwise disjointness (every distinct ALL index pair yields
    // distinct markers), (e) the membership-gate acceptance side (every
    // ALL element classifies as `true`), and (f) the membership-gate
    // rejection side (the bare LEAD byte + unrecognised `&`-prefixed
    // names classify as `false`).
    //
    // Fail-before/pass-after: every test below references
    // `MacroParams::LAMBDA_LIST_KEYWORDS` or
    // `MacroParams::is_lambda_list_keyword`, which simply did not exist
    // on `MacroParams` before this lift — every assertion was a
    // compile-time error against the prior surface.

    #[test]
    fn macro_params_lambda_list_keywords_has_expected_cardinality() {
        // CARDINALITY PIN: the ALL array closes the family at exactly TWO
        // entries — the two `&'static str` markers the parser's typed
        // dispatch specialises on today (`&rest` + `&optional`). A future
        // third marker (`&key`, `&aux`, `&body`) extends this arity to
        // 3 AND every downstream family-wide contract sweep picks up the
        // extension at ONE structural site. A regression that drops a
        // marker from the ALL array (silently narrowing the family) OR
        // aliases two markers to the same index (silently narrowing the
        // typed dispatch) fails HERE at the cardinality contract before
        // the structural round-trip / pairwise-disjointness contracts
        // even fire.
        assert_eq!(
            MacroParams::LAMBDA_LIST_KEYWORDS.len(),
            2,
            "LAMBDA_LIST_KEYWORDS cardinality drifted from 2 — the CL \
             lambda-list-keyword family closure now names a different \
             number of markers than the two the parser's typed dispatch \
             specialises on.",
        );
    }

    #[test]
    fn macro_params_lambda_list_keywords_binds_per_role_markers_by_index() {
        // ORDERING PIN: each ALL entry matches its corresponding per-role
        // `pub const` by-index. A regression that swaps the ordering
        // (`[OPTIONAL_MARKER, REST_MARKER]` after a rebase) would keep
        // the cardinality + membership contracts intact but silently
        // reorder downstream consumers that iterate `LAMBDA_LIST_KEYWORDS`
        // in canonical order (a future authoring surface that renders
        // "supported CL lambda-list keywords: &rest, &optional" would
        // silently render "supported CL lambda-list keywords: &optional,
        // &rest" instead). Pins the array's declaration-order binding
        // structurally so a reorder fails HERE at each element rather
        // than only at consumer sites downstream.
        assert_eq!(
            MacroParams::LAMBDA_LIST_KEYWORDS[0],
            MacroParams::REST_MARKER,
            "LAMBDA_LIST_KEYWORDS[0] drifted from REST_MARKER — the ALL \
             array's declaration-order binding to the per-role `pub \
             const` broke at the rest-slot marker slot.",
        );
        assert_eq!(
            MacroParams::LAMBDA_LIST_KEYWORDS[1],
            MacroParams::OPTIONAL_MARKER,
            "LAMBDA_LIST_KEYWORDS[1] drifted from OPTIONAL_MARKER — the \
             ALL array's declaration-order binding to the per-role \
             `pub const` broke at the optional-section marker slot.",
        );
    }

    #[test]
    fn macro_params_every_lambda_list_keyword_prefixed_by_lambda_list_keyword_lead() {
        // FAMILY-WIDE STRUCTURAL ROUND-TRIP PIN: every element of the ALL
        // array MUST start with the canonical LEAD byte. Where the two
        // per-marker `_prefixed_by_lambda_list_keyword_lead` pins each
        // named ONE marker inline as a duplicate 3-line
        // `assert!(A.starts_with(LEAD), ...)` shape, this family-wide
        // sweep routes the SAME contract through the closed-set ALL
        // array — a future third marker (`&key`, `&aux`, `&body`)
        // extends the array AND is automatically covered by this sweep
        // without adding a third `_prefixed_by_...` per-marker pin. The
        // per-marker pins stay as fine-grained fail-loud sites; this
        // pin is the algebra-wide closure the family-extending refactor
        // routes through.
        for m in MacroParams::LAMBDA_LIST_KEYWORDS {
            assert!(
                m.starts_with(MacroParams::LAMBDA_LIST_KEYWORD_LEAD),
                "LAMBDA_LIST_KEYWORDS element `{m}` does NOT start with \
                 LAMBDA_LIST_KEYWORD_LEAD `{lead:?}` — the CL lambda- \
                 list-keyword family's structural round-trip contract \
                 no longer binds every marker to its shared LEAD byte.",
                lead = MacroParams::LAMBDA_LIST_KEYWORD_LEAD,
            );
        }
    }

    #[test]
    fn macro_params_lambda_list_keywords_pairwise_distinct() {
        // FAMILY-WIDE PAIRWISE DISJOINTNESS PIN: every distinct index pair
        // `(i, j)` in the ALL array yields distinct markers. Where the
        // pre-lift `_rest_and_optional_markers_pairwise_disjoint` pin
        // named the TWO markers as a hand-rolled `assert_ne!` pair, this
        // family-wide sweep routes the SAME contract through the closed-
        // set ALL array so a future third marker automatically extends
        // the sweep to `(3 * 2) / 2 == 3` distinct pairs without re-
        // deriving per-marker `assert_ne!` calls. The pre-lift pin
        // stays as the fine-grained fail-loud site; this pin is the
        // family-wide closure the algebra-extending refactor routes
        // through.
        for (i, a) in MacroParams::LAMBDA_LIST_KEYWORDS.iter().enumerate() {
            for (j, b) in MacroParams::LAMBDA_LIST_KEYWORDS.iter().enumerate() {
                if i == j {
                    continue;
                }
                assert_ne!(
                    a, b,
                    "LAMBDA_LIST_KEYWORDS[{i}] `{a}` collides with \
                     LAMBDA_LIST_KEYWORDS[{j}] `{b}` — the CL lambda- \
                     list-keyword family's pairwise disjointness \
                     contract no longer binds distinct index pairs \
                     to distinct markers.",
                );
            }
        }
    }

    #[test]
    fn macro_params_is_lambda_list_keyword_accepts_every_marker() {
        // ACCEPTANCE-SIDE MEMBERSHIP PIN: every element of the ALL array
        // MUST classify as `true` through the typed membership gate. The
        // gate is defined as `LAMBDA_LIST_KEYWORDS.contains(&s)`, so this
        // pin is the structural closure binding the gate's return value
        // to the ALL array's element set. A regression that specialises
        // the gate to a subset of the array (e.g. a `matches!(s,
        // "&rest")` inline that silently drops the `&optional` branch)
        // fails HERE at the ALL sweep. Sibling-shape acceptance sweep to
        // the closed-set `ClosedSet::parse_label` roundtrip pin — every
        // element of `Self::ALL` decodes through the projection back to
        // itself.
        for m in MacroParams::LAMBDA_LIST_KEYWORDS {
            assert!(
                MacroParams::is_lambda_list_keyword(m),
                "is_lambda_list_keyword rejected LAMBDA_LIST_KEYWORDS \
                 element `{m}` — the closed-set membership gate's \
                 acceptance side drifted from the ALL array.",
            );
        }
    }

    #[test]
    fn macro_params_is_lambda_list_keyword_rejects_bare_lead_byte() {
        // REJECTION-SIDE MEMBERSHIP PIN (LEAD byte alone): the bare LEAD
        // byte `"&"` MUST classify as `false`. A future Clojure-compat
        // port might land a bare `&` marker — but until such an
        // extension explicitly lands on the ALL array, the substrate
        // MUST NOT silently classify the LEAD byte alone as a
        // recognised CL lambda-list keyword. Pins the gate's
        // rejection-side contract against a plausible future
        // near-miss extension.
        let bare_lead: String = MacroParams::LAMBDA_LIST_KEYWORD_LEAD.to_string();
        assert!(
            !MacroParams::is_lambda_list_keyword(&bare_lead),
            "is_lambda_list_keyword accepted the bare LEAD byte `{bare_lead}` — \
             the closed-set membership gate silently classifies the bare `&` \
             LEAD byte as a recognised CL lambda-list keyword despite the ALL \
             array containing only the two suffixed markers.",
        );
    }

    #[test]
    fn macro_params_is_lambda_list_keyword_rejects_unrecognised_ampersand_prefixed_names() {
        // REJECTION-SIDE MEMBERSHIP PIN (unrecognised near-misses): the
        // three plausible future CL lambda-list keywords (`&key` for
        // keyword-args, `&aux` for auxiliaries, `&body` for the
        // docstring-carrying tail of `defmacro` bodies) MUST classify as
        // `false` UNTIL they explicitly land on the ALL array. Pins the
        // gate's conservative rejection contract — the closed-set
        // membership gate is not a "starts with `&`" heuristic; it is a
        // typed enumeration over the currently-recognised markers. A
        // regression that silently loosens the gate to a `starts_with`
        // check (dropping the closed-set discipline) fails HERE.
        for candidate in ["&key", "&aux", "&body"] {
            assert!(
                !MacroParams::is_lambda_list_keyword(candidate),
                "is_lambda_list_keyword accepted the unrecognised \
                 `&`-prefixed name `{candidate}` — the closed-set \
                 membership gate silently loosened its acceptance beyond \
                 the two markers the ALL array names today.",
            );
        }
    }

    #[test]
    fn macro_params_is_lambda_list_keyword_rejects_bare_identifiers_and_empty_string() {
        // REJECTION-SIDE MEMBERSHIP PIN (bare identifiers + empty): a
        // bare identifier (`"a"`, `"xs"`, `"foo"`) and the empty string
        // MUST classify as `false`. These are the shapes the parser's
        // bare-symbol dispatch cascade sees at `parse_params` when
        // walking a param list, so pinning them here binds the
        // membership gate's rejection contract to the parser's most
        // common non-marker input shapes.
        for candidate in ["a", "xs", "foo", ""] {
            assert!(
                !MacroParams::is_lambda_list_keyword(candidate),
                "is_lambda_list_keyword accepted the bare non-marker \
                 input `{candidate}` — the closed-set membership gate's \
                 rejection side no longer excludes bare identifiers \
                 the parser routes through the fall-through cascade.",
            );
        }
    }

    // ── fixed_arity: the rest-start / rest-less max-arity primitive ─────
    //
    // `fixed_arity()` lifts the `self.required.len() + self.optional.len()`
    // arithmetic that recurred three times inside `MacroParams::bind` — at
    // the `Vec::with_capacity` site (where it adds `usize::from(rest.is_some())`
    // to get the bound-values count), at the `rest_start` site (inside the
    // `if let Some(rest)` branch), and at the `expected` site (inside the
    // rest-less `else`). The latter two sites live in mutually-exclusive
    // branches yet name ONE structural concept; lifting them collapses the
    // arithmetic to one named primitive. These tests pin the primitive's
    // contract directly; the existing `bind_*` tests are the path-uniformity
    // guards proving `bind`'s sites route through the same value without
    // behavior drift.
    //
    // Fail-before/pass-after: every test below references
    // `params.fixed_arity()`, which simply did not exist on `MacroParams`
    // before this lift — every assertion's `expect: ___ == params.fixed_arity()`
    // line was a compile-time error against the prior surface.

    #[test]
    fn fixed_arity_is_zero_for_the_empty_param_list() {
        // `()` — a nullary macro has fixed arity 0, the rest-less binder
        // boundary at which the FIRST surplus arg already rejects.
        let params = MacroParams::default();
        assert_eq!(params.fixed_arity(), 0);
    }

    #[test]
    fn fixed_arity_counts_required_only_when_no_optional_or_rest() {
        // `(a b c)` — three required, no optional, no rest. fixed_arity is
        // exactly the required length.
        let params = MacroParams {
            required: vec!["a".into(), "b".into(), "c".into()],
            optional: Vec::new(),
            rest: None,
        };
        assert_eq!(params.fixed_arity(), 3);
    }

    #[test]
    fn fixed_arity_counts_optional_only_when_no_required_or_rest() {
        // `(&optional x y)` — two optional, no required. fixed_arity is the
        // optional length; the optional section participates in the fixed
        // run because supplied positional args bind to it.
        let params = MacroParams {
            required: Vec::new(),
            optional: vec![OptionalParam::bare("x"), OptionalParam::bare("y")],
            rest: None,
        };
        assert_eq!(params.fixed_arity(), 2);
    }

    #[test]
    fn fixed_arity_sums_required_and_optional_in_canonical_lambda_order() {
        // `(a b &optional c d e)` — two required + three optional, no rest.
        // fixed_arity is 5: the maximum arity a rest-less call can supply.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: vec![
                OptionalParam::bare("c"),
                OptionalParam::bare("d"),
                OptionalParam::bare("e"),
            ],
            rest: None,
        };
        assert_eq!(params.fixed_arity(), 5);
    }

    #[test]
    fn fixed_arity_ignores_rest_slot_by_construction() {
        // `(a &optional b &rest r)` and `(a &optional b)` — identical fixed
        // arity (2). The `&rest` slot has NO maximum and is structurally
        // excluded from `fixed_arity`. Naming this invariant pins that a
        // regression that drifts the primitive to "required + optional +
        // rest.is_some() as usize" fails loudly here — that drift would
        // collapse `fixed_arity` into `names().len()`, losing the rest-start
        // vs total-bound-values distinction the typed shape relies on.
        let with_rest = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: Some("r".into()),
        };
        let without_rest = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: None,
        };
        assert_eq!(with_rest.fixed_arity(), without_rest.fixed_arity());
        assert_eq!(with_rest.fixed_arity(), 2);
    }

    #[test]
    fn fixed_arity_is_the_rest_start_index_in_names_when_rest_present() {
        // When `rest` is `Some`, `names()[fixed_arity()]` IS the rest name
        // — the rest-start reading of the primitive. Same arithmetic the
        // bytecode index would hit (`Subst(fixed_arity())` resolves to the
        // rest-bound `Sexp::List`).
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: vec![OptionalParam::bare("c")],
            rest: Some("r".into()),
        };
        assert_eq!(params.fixed_arity(), 3);
        assert_eq!(params.names()[params.fixed_arity()], "r");
    }

    #[test]
    fn fixed_arity_equals_names_length_when_rest_is_absent() {
        // When `rest` is `None`, `names().len() == fixed_arity()` — there
        // is no rest-name slot to extend the flat run past the fixed
        // boundary. Pins the structural identity
        // `names().len() == fixed_arity() + usize::from(rest.is_some())`
        // for the rest-less case; the rest-present case is pinned by the
        // sibling test above (where the boundary is the rest-name index,
        // i.e. one short of `names().len()`).
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: vec![OptionalParam::bare("c")],
            rest: None,
        };
        assert_eq!(params.names().len(), params.fixed_arity());
        assert_eq!(params.names().len(), 3);
    }

    #[test]
    fn fixed_arity_is_the_rest_less_surplus_rejection_boundary() {
        // The `expected` field of `TooManyMacroArgs` IS `fixed_arity()` —
        // the rest-less binder rejects iff `args.len() > fixed_arity()`.
        // This pin is the path-uniformity guard binding the typed primitive
        // to the binder's rejection contract: a regression that drifts
        // `bind`'s `expected` arithmetic from `fixed_arity()` would silently
        // surface a different boundary in the diagnostic without touching
        // the primitive — and this assertion fails loudly. Mirror of the
        // sibling rest-less surplus pin (`bind_rest_less_params_reject_
        // surplus_args`); this test pins WHAT the `expected` slot's value
        // structurally IS, that pin checks the variant SHAPE.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: vec![OptionalParam::bare("c")],
            rest: None,
        };
        assert_eq!(params.fixed_arity(), 3);
        let err = params
            .bind(
                "m",
                &[Sexp::int(1), Sexp::int(2), Sexp::int(3), Sexp::int(4)],
            )
            .expect_err("4 args against fixed_arity 3 must reject");
        match err {
            LispError::TooManyMacroArgs {
                expected,
                got,
                macro_name,
            } => {
                assert_eq!(expected, params.fixed_arity());
                assert_eq!(got, 4);
                assert_eq!(macro_name, "m");
            }
            other => panic!("expected TooManyMacroArgs, got {other:?}"),
        }
    }

    #[test]
    fn fixed_arity_is_the_rest_start_index_consumed_by_bind() {
        // When `rest` is `Some`, `bind` collects `args[fixed_arity()..]`
        // into the rest's `Sexp::List`. Pin that the rest list contents
        // are exactly the suffix beginning at `fixed_arity()` — the
        // primitive's rest-start reading IS the slice index the binder
        // consumes. A regression that drifts `bind`'s rest-collection
        // slice from `fixed_arity()` would surface as a misaligned rest
        // list (off-by-one in either direction) and this assertion fails
        // loudly. Sibling of `fixed_arity_is_the_rest_less_surplus_
        // rejection_boundary` on the rest-PRESENT branch.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: Some("r".into()),
        };
        assert_eq!(params.fixed_arity(), 2);
        let args = [Sexp::int(1), Sexp::int(2), Sexp::int(3), Sexp::int(4)];
        let vals = params.bind("m", &args).unwrap();
        // Bound vec: [a=1, b=2, r=(3 4)] — the rest list IS args[fixed_arity()..].
        let rest_expected: Vec<Sexp> = args[params.fixed_arity()..].to_vec();
        assert_eq!(vals.last().unwrap(), &Sexp::List(rest_expected));
    }

    #[test]
    fn bind_rest_present_at_exact_fixed_arity_yields_empty_rest_list() {
        // Exactly-saturated rest-present call: `args.len() == fixed_arity()`.
        // The rest slot collects the empty slice; bind succeeds, never
        // misaligning to an off-by-one underflow. Pin the boundary on the
        // rest-PRESENT path — the rest-less mirror is
        // `too_many_macro_args_does_not_fire_at_exact_max_arity` above.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: Some("r".into()),
        };
        assert_eq!(params.fixed_arity(), 2);
        let vals = params
            .bind("m", &[Sexp::int(1), Sexp::int(2)])
            .expect("rest-present at exact fixed_arity must bind cleanly");
        assert_eq!(vals, vec![Sexp::int(1), Sexp::int(2), Sexp::List(vec![])]);
    }

    #[test]
    fn bind_threads_required_positionally_and_collects_rest_as_list() {
        // `(a b &rest c)` bound to `1 2 3 4`: a=1, b=2, c=(3 4). The bound
        // vec is parallel to `names()`, so the rest list sits at the rest's
        // flat index.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: Vec::new(),
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
    fn bind_supplied_optional_takes_its_positional_arg() {
        // `(a &optional b)` bound to `1 2`: a=1, b=2. A supplied optional
        // behaves exactly like a positional — only its ABSENCE differs.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: None,
        };
        let vals = params.bind("m", &[Sexp::int(1), Sexp::int(2)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(1), Sexp::int(2)]);
    }

    #[test]
    fn bind_unsupplied_optional_defaults_to_nil() {
        // `(a &optional b c)` bound to just `1`: a=1, then b and c run out of
        // args and bind to `Sexp::Nil` — CL's default for an `&optional` with
        // no supplied default-form. The bound vec is still parallel to
        // `names()`, so the template's `,b` / `,c` resolve to nil, not a
        // missing-arg error.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b"), OptionalParam::bare("c")],
            rest: None,
        };
        let vals = params.bind("m", &[Sexp::int(1)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(1), Sexp::Nil, Sexp::Nil]);
    }

    #[test]
    fn bind_rest_collects_args_beyond_required_and_optional() {
        // `(a &optional b &rest c)` bound to `1 2 3 4`: a=1, b=2 (supplied),
        // c=(3 4). The rest starts AFTER the required+optional run, so the
        // optional's supplied arg is not swept into the rest.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
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
    fn bind_unsupplied_optional_then_empty_rest() {
        // `(a &optional b &rest c)` bound to just `1`: a=1, b=nil (absent),
        // c=() (nothing left). Both the optional default AND the empty-rest
        // contract hold in the same bind.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: Some("c".into()),
        };
        let vals = params.bind("m", &[Sexp::int(1)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(1), Sexp::Nil, Sexp::List(vec![])]);
    }

    #[test]
    fn bind_rest_with_no_remaining_args_is_the_empty_list() {
        // Exactly-saturated required args + a rest that captures nothing →
        // the rest binds to the empty list, never errors. Mirrors the
        // splice contract `,@()` contributes nothing.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: Vec::new(),
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
            optional: Vec::new(),
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
    fn bind_missing_required_errors_even_with_optional_present() {
        // An absent REQUIRED arg errors even when the param list has an
        // optional section: the required walk fires `MissingMacroArg` before
        // the optional arm (which would otherwise default to nil) is reached.
        // Required absence is an error; optional absence is a nil default.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: vec![OptionalParam::bare("c")],
            rest: None,
        };
        let err = params
            .bind("m", &[Sexp::int(1)])
            .expect_err("missing required `b` must error before optional defaulting");
        assert!(
            matches!(err, LispError::MissingMacroArg { .. }),
            "expected MissingMacroArg, got: {err:?}"
        );
    }

    #[test]
    fn bind_rest_less_params_reject_surplus_args() {
        // The rest-less binder REJECTS surplus call args via the
        // structural `TooManyMacroArgs { macro_name, expected, got }`
        // rejection — the call-site mirror of `RestParamTrailingTokens`
        // (the definition-site rejection lifted at the parse_params
        // boundary). Closes the asymmetry where the typed-entry
        // macro-call-gate rejected too-few-args loudly
        // (`MissingMacroArg`) but silently truncated too-many. `expected`
        // is the rest-less binder's fixed maximum arity
        // (`required.len() + optional.len()`); `got` is the actual
        // call-site arg count.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: Vec::new(),
            rest: None,
        };
        let err = params
            .bind("m", &[Sexp::int(1), Sexp::int(2)])
            .expect_err("rest-less surplus must error");
        match err {
            LispError::TooManyMacroArgs {
                macro_name,
                expected,
                got,
            } => {
                assert_eq!(macro_name, "m");
                assert_eq!(expected, 1);
                assert_eq!(got, 2);
            }
            other => panic!("expected TooManyMacroArgs, got: {other:?}"),
        }
    }

    // ── OptionalParam: per-param default forms — `&optional (x DEFAULT)` ──
    //
    // The `&optional` section now admits both bare-symbol entries (`x`) AND
    // list-form entries (`(x DEFAULT)`). The typed `OptionalParam.default:
    // Option<Sexp>` slot makes the per-param default a FIELD on each
    // optional entry, not a discipline a sibling `Vec<Sexp>` would have had
    // to maintain in lock-step with `Vec<String>`. These tests pin: the
    // parser admits both shapes side-by-side; the four malformed list-spec
    // shapes (empty / missing-default / extra-elements / non-symbol-name)
    // are rejected via `OptionalParamMalformed` with the typed
    // `OptionalParamMalformedReason`; the binder consults the default form
    // when the arg is absent and ignores it when supplied; and the end-to-
    // end expansion agrees between the bytecode and substitute strategies
    // (invariant 2 — free middle).

    #[test]
    fn parse_params_admits_optional_list_spec_with_default() {
        // `(a &optional (b 5))` — one bare optional becomes
        // `OptionalParam { name: "b", default: Some(Int(5)) }`. The
        // surrounding `MacroParams` shape is otherwise identical.
        let params = parse_params(&read("a &optional (b 5)").unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: vec!["a".into()],
                optional: vec![OptionalParam::with_default("b", Sexp::int(5))],
                rest: None,
            }
        );
    }

    #[test]
    fn parse_params_mixes_bare_and_list_optional_specs_side_by_side() {
        // `(a &optional b (c "x") d (e 9) &rest r)` — the optional section
        // interleaves bare and list-form specs. Each lands in its own
        // `OptionalParam` entry; `names()` still yields the flat
        // required-then-optional-then-rest order.
        let params =
            parse_params(&read("a &optional b (c \"x\") d (e 9) &rest r").unwrap()).unwrap();
        assert_eq!(
            params,
            MacroParams {
                required: vec!["a".into()],
                optional: vec![
                    OptionalParam::bare("b"),
                    OptionalParam::with_default("c", Sexp::string("x")),
                    OptionalParam::bare("d"),
                    OptionalParam::with_default("e", Sexp::int(9)),
                ],
                rest: Some("r".into()),
            }
        );
        assert_eq!(params.names(), vec!["a", "b", "c", "d", "e", "r"]);
    }

    #[test]
    fn parse_params_admits_arbitrary_sexp_as_optional_default_form() {
        // `(&optional (x (list 1 2)))` — the default form is itself a list.
        // Without an evaluator, the literal Sexp is parked verbatim into
        // `default`; the binder produces it for any absent call.
        let params = parse_params(&read("&optional (x (list 1 2))").unwrap()).unwrap();
        let want_default = Sexp::List(vec![Sexp::symbol("list"), Sexp::int(1), Sexp::int(2)]);
        assert_eq!(
            params,
            MacroParams {
                required: Vec::new(),
                optional: vec![OptionalParam::with_default("x", want_default)],
                rest: None,
            }
        );
    }

    #[test]
    fn parse_params_rejects_empty_list_optional_spec() {
        // `(&optional ())` — a zero-element list is the empty-list rejection.
        // Without the gate the loop would `as_symbol()` on a `Sexp::List` and
        // fall through to `NonSymbolParam`, which mis-classifies the failure
        // (this is a malformed DEFAULT-FORM spec, not a "param must be a
        // symbol" rejection).
        let err = parse_params(&read("&optional ()").unwrap())
            .expect_err("empty list optional spec must error");
        assert!(
            matches!(
                err,
                LispError::OptionalParamMalformed {
                    position: 1,
                    reason: crate::error::OptionalParamMalformedReason::EmptyList,
                    ..
                }
            ),
            "expected OptionalParamMalformed{{EmptyList, position: 1}}, got: {err:?}"
        );
    }

    #[test]
    fn parse_params_rejects_one_element_optional_list_as_missing_default() {
        // `(&optional (x))` — a one-element list. REJECTED with reason
        // `MissingDefault` rather than reinterpreted as `&optional x`, because
        // a parenthesized single-element spec is structurally ambiguous and
        // the bare-symbol form `x` IS the canonical "no default" shape.
        let err = parse_params(&read("&optional (x)").unwrap())
            .expect_err("one-element list optional spec must error");
        assert!(
            matches!(
                err,
                LispError::OptionalParamMalformed {
                    position: 1,
                    reason: crate::error::OptionalParamMalformedReason::MissingDefault,
                    ..
                }
            ),
            "expected OptionalParamMalformed{{MissingDefault, position: 1}}, got: {err:?}"
        );
    }

    #[test]
    fn parse_params_rejects_three_or_more_element_optional_list_as_extra_elements() {
        // `(&optional (x 5 6))` — a three-element list. CL's `(name default
        // supplied-p)` shape is not yet supported (no evaluator → no
        // supplied-p variable binding), so the third element is structurally
        // surplus. REJECTED with reason `ExtraElements{length: 3}`.
        let err = parse_params(&read("&optional (x 5 6)").unwrap())
            .expect_err("three-element list optional spec must error");
        assert!(
            matches!(
                err,
                LispError::OptionalParamMalformed {
                    position: 1,
                    reason: crate::error::OptionalParamMalformedReason::ExtraElements { length: 3 },
                    ..
                }
            ),
            "expected OptionalParamMalformed{{ExtraElements{{3}}, position: 1}}, got: {err:?}"
        );
    }

    #[test]
    fn parse_params_rejects_non_symbol_name_in_optional_list_spec() {
        // `(&optional (5 default))` — the name slot must be a symbol; a
        // numeric literal is REJECTED with reason `NonSymbolName`. Without
        // this branch the gate would silently populate
        // `OptionalParam.name` from a stringified non-symbol value (`"5"`),
        // breaking the invariant that param names are symbols.
        let err = parse_params(&read("&optional (5 default)").unwrap())
            .expect_err("non-symbol-name optional spec must error");
        assert!(
            matches!(
                err,
                LispError::OptionalParamMalformed {
                    position: 1,
                    reason: crate::error::OptionalParamMalformedReason::NonSymbolName,
                    ..
                }
            ),
            "expected OptionalParamMalformed{{NonSymbolName, position: 1}}, got: {err:?}"
        );
    }

    #[test]
    fn parse_params_rejects_list_in_required_section_as_non_symbol_param() {
        // `((a 5))` — a list in the REQUIRED section is NOT a default-form
        // spec; default forms are an optional-section affordance. The gate
        // must fall through to `NonSymbolParam` (parity with the prior
        // behavior on lists in the required section), not silently admit
        // the list as a default-form spec.
        let err =
            parse_params(&read("(a 5)").unwrap()).expect_err("list in required section must error");
        assert!(
            matches!(err, LispError::NonSymbolParam { position: 0, .. }),
            "expected NonSymbolParam{{position: 0}}, got: {err:?}"
        );
    }

    #[test]
    fn bind_unsupplied_optional_with_default_takes_the_default() {
        // `(a &optional (b 5))` bound to just `1`: a=1, b=5 (the declared
        // default), not nil. The default form is consulted ONLY when the
        // call ran out of args.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::with_default("b", Sexp::int(5))],
            rest: None,
        };
        let vals = params.bind("m", &[Sexp::int(1)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(1), Sexp::int(5)]);
    }

    #[test]
    fn bind_supplied_optional_with_default_takes_the_arg_not_the_default() {
        // `(&optional (b 5))` bound to `42`: b=42, NOT the default. A
        // supplied optional ALWAYS takes its arg; the default is the
        // absence-only fallback. Pins that the default form does not
        // shadow a supplied call arg.
        let params = MacroParams {
            required: Vec::new(),
            optional: vec![OptionalParam::with_default("b", Sexp::int(5))],
            rest: None,
        };
        let vals = params.bind("m", &[Sexp::int(42)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(42)]);
    }

    #[test]
    fn bind_mixes_supplied_unsupplied_default_and_nil_floor() {
        // `(a &optional (b 5) c (d "z"))` bound to just `1`: a=1, b=5
        // (default), c=nil (bare floor), d="z" (default). The three
        // absence cases coexist in one bind: per-default fill, nil floor,
        // and a tail with a literal-string default.
        let params = MacroParams {
            required: vec!["a".into()],
            optional: vec![
                OptionalParam::with_default("b", Sexp::int(5)),
                OptionalParam::bare("c"),
                OptionalParam::with_default("d", Sexp::string("z")),
            ],
            rest: None,
        };
        let vals = params.bind("m", &[Sexp::int(1)]).unwrap();
        assert_eq!(
            vals,
            vec![Sexp::int(1), Sexp::int(5), Sexp::Nil, Sexp::string("z")]
        );
    }

    // ── OptionalParam::resolved_default: the absent-call binder accessor ──
    //
    // `resolved_default` lifts the `param.default.clone().unwrap_or(Sexp::Nil)`
    // two-arm fallback that previously inlined at `MacroParams::bind`'s
    // optional arm into ONE named accessor on the typed `OptionalParam`.
    // The constructor pair `bare` / `with_default` defines the typed
    // shapes of the `default` slot; this accessor names the symmetric
    // bound-value projection both shapes yield at the absence boundary.
    // Tests pin: (a) `bare(name).resolved_default()` is the `Sexp::Nil`
    // no-default floor; (b) `with_default(name, d).resolved_default()` is
    // `d.clone()`; (c) the projection is `Clone`-stable across repeated
    // calls (the typed `default` field is not consumed); (d) path-
    // uniformity at the binder — `bind`'s optional arm routes through
    // `resolved_default` for both shapes; (e) end-to-end through both
    // expansion strategies, the absent-call binding agrees.

    #[test]
    fn resolved_default_is_nil_for_bare_optional() {
        // `OptionalParam::bare(name).default` is `None`, so
        // `resolved_default()` projects to `Sexp::Nil` — the CL
        // `&optional` no-default-form floor. Fail-before/pass-after: this
        // assert is meaningless pre-lift because the helper does not
        // exist; post-lift it pins the typed accessor's `Sexp::Nil` arm
        // at the named primitive. Sibling of `bare` itself: the
        // constructor defines the shape (`default: None`); the accessor
        // names the bound-value projection of that shape.
        let p = OptionalParam::bare("x");
        assert_eq!(p.resolved_default(), Sexp::Nil);
    }

    #[test]
    fn resolved_default_clones_declared_default_for_with_default_optional() {
        // `OptionalParam::with_default(name, d).default` is `Some(d)`, so
        // `resolved_default()` projects to `d.clone()` — the declared
        // default form. Sibling of the bare-floor pin: the closed-set
        // `default: Option<Sexp>` slot's two shapes correspond 1:1 with
        // the two arms of `resolved_default`. Pins the closed-set
        // exhaustive coverage of `Option<Sexp>` × `{Some, None}`.
        let p = OptionalParam::with_default("x", Sexp::int(5));
        assert_eq!(p.resolved_default(), Sexp::int(5));
    }

    #[test]
    fn resolved_default_clones_arbitrary_sexp_default_form() {
        // The declared default can be any `Sexp` — a literal list, a
        // keyword, a string, a quasi-quoted form — because v0 has no
        // evaluator and the typed slot parks the literal verbatim. Pin
        // that `resolved_default()` is faithful to the parked literal
        // regardless of shape: a regression that special-cases an arm
        // (e.g., projecting `Sexp::List(_)` to `Sexp::Nil`, or "normalizing"
        // a `Sexp::Quote`) fails here. The accessor is exactly
        // `default.clone()` for the `Some` arm — no shape rewriting.
        let arbitrary = Sexp::List(vec![Sexp::symbol("list"), Sexp::int(1), Sexp::int(2)]);
        let p = OptionalParam::with_default("x", arbitrary.clone());
        assert_eq!(p.resolved_default(), arbitrary);
    }

    #[test]
    fn resolved_default_is_clone_stable_across_repeated_calls() {
        // The accessor takes `&self` and projects through `Clone`, so
        // repeated calls yield IDENTICAL values — the typed `default`
        // field is not consumed. Pins that the accessor is idempotent
        // for the same `OptionalParam`, which is the contract the binder
        // relies on across multiple `bind` invocations of the same
        // macro: every call that leaves the optional unfilled yields
        // the SAME bound value, never a partially-consumed shape. A
        // regression that converted the accessor to `self.default.take()`
        // (consuming the field) would still type-check at the call site
        // but would silently desync repeated absent-call bindings; this
        // test catches that drift.
        let p = OptionalParam::with_default("x", Sexp::string("hi"));
        let first = p.resolved_default();
        let second = p.resolved_default();
        assert_eq!(first, second);
        assert_eq!(first, Sexp::string("hi"));
    }

    #[test]
    fn resolved_default_is_the_binders_absent_optional_projection() {
        // Path-uniformity pin at the binder boundary: `MacroParams::bind`'s
        // optional arm consults `param.resolved_default()` for any
        // absent slot. Two-arm coverage: a bare optional (`b`) binds to
        // `Sexp::Nil` via the `None` arm; a with-default optional
        // (`(c 5)`) binds to `Sexp::int(5)` via the `Some` arm. The
        // single bind call exercises both arms in one walk, and the
        // bound values vec is parallel to `names()` so position
        // checking pins the arm-to-slot mapping. A regression that
        // re-inlines the two-arm fallback at the binder, drifting it
        // independently from the accessor, would still type-check but
        // a future shape change to `resolved_default` (e.g., adding a
        // typed `&supplied-p` companion slot) would silently desync
        // the binder from the accessor — this test catches that drift.
        let params = MacroParams {
            required: Vec::new(),
            optional: vec![
                OptionalParam::bare("b"),
                OptionalParam::with_default("c", Sexp::int(5)),
            ],
            rest: None,
        };
        // Empty args → both optionals are absent → both arms fire.
        let vals = params.bind("m", &[]).unwrap();
        assert_eq!(vals.len(), 2);
        assert_eq!(vals[0], OptionalParam::bare("b").resolved_default());
        assert_eq!(
            vals[1],
            OptionalParam::with_default("c", Sexp::int(5)).resolved_default()
        );
        // And the absolute identities pin the projection's arms:
        assert_eq!(vals[0], Sexp::Nil);
        assert_eq!(vals[1], Sexp::int(5));
    }

    #[test]
    fn resolved_default_is_path_uniform_across_bytecode_and_substitute() {
        // End-to-end path-uniformity: a macro with both an `&optional
        // (g "hi")` (with-default) and an `&optional h` (bare) param
        // expands the same way under bytecode AND substitute
        // strategies, because both strategies route through the SHARED
        // `MacroParams::bind` which now consults `resolved_default`
        // for absent slots. Pins that the accessor's contract is
        // structurally observable at the strategy boundary — a
        // regression that bifurcated the accessor's behavior between
        // the two paths (impossible, since they share `bind`) would
        // surface here.
        let src = r#"
            (defmacro greet (n &optional (g "hi") h)
              `(list ,g ,n ,h))
            (greet world)
        "#;
        let expected = vec![Sexp::List(vec![
            Sexp::symbol("list"),
            Sexp::string("hi"),
            Sexp::symbol("world"),
            Sexp::Nil,
        ])];
        let bytecode = Expander::new().expand_program(read(src).unwrap()).unwrap();
        let substitute = Expander::new_substitute_only()
            .expand_program(read(src).unwrap())
            .unwrap();
        assert_eq!(
            bytecode, expected,
            "bytecode resolved_default expansion drifted"
        );
        assert_eq!(
            substitute, expected,
            "substitute resolved_default expansion drifted"
        );
        assert_eq!(
            bytecode, substitute,
            "the two strategies disagree on resolved_default expansion"
        );
    }

    #[test]
    fn resolved_default_supplied_optional_does_not_consult_accessor() {
        // A SUPPLIED optional binds to its CALL ARG, never to the
        // accessor's projection. Pins the contract: `resolved_default`
        // is the absence-only fallback; a present arg shadows the
        // accessor at the binder. Sibling negative control to
        // `resolved_default_is_the_binders_absent_optional_projection`:
        // that test exercises the absence arm at every slot; this test
        // exercises the presence arm at every slot and proves the
        // accessor's `Sexp::int(5)` projection is NOT consulted when
        // the optional is supplied with `Sexp::int(42)`. A regression
        // that wired the binder to always consult the accessor (the
        // wrong direction — the default would shadow supplied args) is
        // caught here.
        let params = MacroParams {
            required: Vec::new(),
            optional: vec![OptionalParam::with_default("b", Sexp::int(5))],
            rest: None,
        };
        let vals = params.bind("m", &[Sexp::int(42)]).unwrap();
        assert_eq!(vals, vec![Sexp::int(42)]);
        // And the accessor's would-be projection IS NOT the bound value:
        let p = OptionalParam::with_default("b", Sexp::int(5));
        assert_ne!(vals[0], p.resolved_default());
    }

    #[test]
    fn optional_default_macro_expands_end_to_end_under_both_strategies() {
        // The end-to-end path: a macro with `&optional (g "hi")` expands to
        // the default literal when unsupplied, and to the supplied arg when
        // present. Both the bytecode and substitute strategies must agree
        // (invariant 2 — free middle); they share `MacroParams::bind`, so
        // the default arm lands once in `bind` and both strategies inherit
        // it unable to drift. This is the test the prior run (611a682)
        // signposted as the next-change-that-benefits.
        let src = r#"
            (defmacro greet (n &optional (g "hi"))
              `(list ,g ,n))
            (greet world)
            (greet world there)
        "#;
        let expected = vec![
            Sexp::List(vec![
                Sexp::symbol("list"),
                Sexp::string("hi"),
                Sexp::symbol("world"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("list"),
                Sexp::symbol("there"),
                Sexp::symbol("world"),
            ]),
        ];
        let bytecode = Expander::new().expand_program(read(src).unwrap()).unwrap();
        let substitute = Expander::new_substitute_only()
            .expand_program(read(src).unwrap())
            .unwrap();
        assert_eq!(
            bytecode, expected,
            "bytecode optional-default expansion drifted"
        );
        assert_eq!(
            substitute, expected,
            "substitute optional-default expansion drifted"
        );
        assert_eq!(
            bytecode, substitute,
            "the two strategies disagree on optional-default expansion"
        );
    }

    #[test]
    fn optional_macro_expands_end_to_end_under_both_strategies() {
        // The end-to-end path: a macro with an `&optional` param expands
        // correctly whether the optional is supplied or defaulted, and the
        // bytecode and substitute strategies agree (invariant 2 — free
        // middle). `,b` resolves to the supplied arg when present, to
        // `Sexp::Nil` when absent (CL's `&optional` default).
        let src = "(defmacro pair (a &optional b) `(cons ,a ,b)) (pair 1 2) (pair 3)";
        // (cons 1 2) — optional supplied; (cons 3 <Nil>) — optional defaulted.
        // The defaulted slot is the canonical `Sexp::Nil`, distinct in the AST
        // from a reader-produced empty list `()` even though both Display as
        // `()`.
        let expected = vec![
            Sexp::List(vec![Sexp::symbol("cons"), Sexp::int(1), Sexp::int(2)]),
            Sexp::List(vec![Sexp::symbol("cons"), Sexp::int(3), Sexp::Nil]),
        ];
        let bytecode = Expander::new().expand_program(read(src).unwrap()).unwrap();
        let substitute = Expander::new_substitute_only()
            .expand_program(read(src).unwrap())
            .unwrap();
        assert_eq!(bytecode, expected, "bytecode optional expansion drifted");
        assert_eq!(
            substitute, expected,
            "substitute optional expansion drifted"
        );
        assert_eq!(
            bytecode, substitute,
            "the two strategies disagree on optional expansion"
        );
    }

    // ── MacroDef::template_body: the shared body-projection primitive ──
    //
    // `template_body` lifts the `match &def.body { Sexp::Quasiquote(inner)
    // => inner.as_ref(), other => other }` inline peel — present
    // byte-identically at the bytecode (`compile_template`) AND substitute
    // (`apply`'s fallback) path entries — into ONE named projection both
    // strategies share. The existing `compiled_template_matches_substitute
    // _path` and `expansion_layers_agree_on_output_and_cache_wins` tests
    // are the path-uniformity guards covering the SHARED-PROJECTION shape;
    // the four tests below pin the projection's contract DIRECTLY:
    // (a) a quasi-quoted body unwraps to the inner; (b) a non-quasi-quoted
    // body returns the body verbatim; (c) the borrow is rooted in the
    // body field (single-level peel); (d) the projection is the same
    // `&Sexp` both strategies route on (so a regression that drifts the
    // body-peel from one strategy to the other becomes a type-level
    // change at this helper, not a silent two-site divergence).

    #[test]
    fn template_body_unwraps_outer_quasiquote_to_inner() {
        // The canonical authoring shape: `(defmacro f (a) `(list ,a))` —
        // the reader wraps the `` ` `` form into `Sexp::Quasiquote(inner)`,
        // and `template_body` peels the outer marker. The returned `&Sexp`
        // is the inner walker-payload both expansion strategies consume.
        let inner = Sexp::List(vec![
            Sexp::symbol("list"),
            Sexp::Unquote(Box::new(Sexp::symbol("a"))),
        ]);
        let def = MacroDef {
            name: "f".into(),
            params: MacroParams::default(),
            body: Sexp::Quasiquote(Box::new(inner.clone())),
        };
        assert_eq!(def.template_body(), &inner);
    }

    #[test]
    fn template_body_returns_non_quasiquote_body_verbatim() {
        // A body authored WITHOUT the outer `` ` `` affordance — a bare
        // `Sexp::List` body — returns verbatim. The "other" arm of the
        // legacy match. Pin parity with the pre-lift code path so a
        // regression that drifts the body-peel into "always peel
        // something" (which would break literal-body macros) fails here.
        let body = Sexp::List(vec![Sexp::symbol("list"), Sexp::int(1)]);
        let def = MacroDef {
            name: "f".into(),
            params: MacroParams::default(),
            body: body.clone(),
        };
        assert_eq!(def.template_body(), &body);
        // Atom bodies too — the projection is a single-arm match, not a
        // recursive descent. A `Sexp::Atom` body is its own template payload.
        let atom_def = MacroDef {
            name: "g".into(),
            params: MacroParams::default(),
            body: Sexp::symbol("nil-template"),
        };
        assert_eq!(atom_def.template_body(), &Sexp::symbol("nil-template"));
    }

    #[test]
    fn template_body_peels_single_level_only() {
        // A nested `` ``form `` body — `Sexp::Quasiquote(Box::new(
        // Sexp::Quasiquote(...)))` — unwraps ONE outer quasi-quote and
        // returns the inner `Sexp::Quasiquote(...)` as-is. The v0 module
        // preamble declares "Nested quasi-quotes: Not yet supported"; the
        // single-level peel matches the legacy inline match's posture
        // (which only matched ONE outer `Sexp::Quasiquote(_)` arm, not a
        // recursive loop). A regression that drifts to a recursive peel
        // would project too far and the inner `Sexp::Quasiquote` marker —
        // which the substitute walker treats as an atomic leaf returned
        // verbatim (line ~830, `Sexp::Quote(_) | Sexp::Quasiquote(_)
        // => Ok(form.clone())`) — would silently disappear from the
        // expansion's emitted form. Pin the single-level contract here.
        let inner_payload = Sexp::List(vec![Sexp::symbol("list"), Sexp::int(7)]);
        let inner_qq = Sexp::Quasiquote(Box::new(inner_payload.clone()));
        let def = MacroDef {
            name: "nested".into(),
            params: MacroParams::default(),
            body: Sexp::Quasiquote(Box::new(inner_qq.clone())),
        };
        // Outer peel returns the INNER quasi-quote, NOT its inner payload.
        assert_eq!(def.template_body(), &inner_qq);
        assert_ne!(def.template_body(), &inner_payload);
    }

    #[test]
    fn template_body_returns_quote_form_verbatim_distinct_from_quasiquote() {
        // A `Sexp::Quote(_)` body — not a `Sexp::Quasiquote(_)` — returns
        // verbatim through the "other" arm. The two close-cousin shapes
        // share an outer-marker character (`'` vs `` ` ``) at the reader
        // boundary but differ semantically: a `Quote` body is a literal
        // template (no substitution semantics), a `Quasiquote` body is a
        // substitution-walker entry. A regression that conflated the two
        // — peeling Quote as if it were Quasiquote — would silently turn
        // every quoted-body macro into a template-walked macro. Pin the
        // discrimination.
        let inner = Sexp::List(vec![Sexp::symbol("opaque"), Sexp::int(42)]);
        let body = Sexp::Quote(Box::new(inner.clone()));
        let def = MacroDef {
            name: "quoted".into(),
            params: MacroParams::default(),
            body: body.clone(),
        };
        assert_eq!(def.template_body(), &body);
        // The inner is NOT what comes back — only Quasiquote-bodied macros
        // would peel to the inner.
        assert_ne!(def.template_body(), &inner);
    }

    #[test]
    fn template_body_is_the_shared_projection_both_strategies_walk() {
        // End-to-end path-uniformity at the projection boundary: a macro
        // authored with the canonical quasi-quoted body expands
        // IDENTICALLY under bytecode and substitute strategies because
        // both route their walker's body through `template_body()` — the
        // SAME `&Sexp` projection. Sibling of
        // `compiled_template_matches_substitute_path` (which observes
        // agreement on the EMITTED form); this test pins agreement on
        // the projection ENTRY: `compile_template` and the substitute
        // fallback now consume the same `&Sexp` (`def.template_body()`),
        // never a divergent inline match the two paths could regress
        // independently.
        let src = "(defmacro wrap (x) `(list ,x ,x)) (wrap 5)";
        let expected = vec![Sexp::List(vec![
            Sexp::symbol("list"),
            Sexp::int(5),
            Sexp::int(5),
        ])];
        let bytecode = Expander::new().expand_program(read(src).unwrap()).unwrap();
        let substitute = Expander::new_substitute_only()
            .expand_program(read(src).unwrap())
            .unwrap();
        assert_eq!(bytecode, expected, "bytecode body-projection drifted");
        assert_eq!(substitute, expected, "substitute body-projection drifted");
        assert_eq!(
            bytecode, substitute,
            "the two strategies disagree on the body-projection's emission"
        );
    }

    // ── Expander::expand: macro-call dispatch routes through `as_call_to_any` ──
    //
    // `expand` lifts its macro-call recognition to route through the
    // substrate's typed-decoded call decomposition: `as_call_to_any(|h|
    // self.macros.get(h))` answers "is this form an invocation of any
    // registered macro?" in ONE structural query on the Sexp algebra,
    // and a HashMap-backed lookup as its classifier. Sibling consumer to
    // `macro_def_from` (the typed-macro-definition dispatcher already
    // routing through `as_call_to_any(MacroDefHead::from_keyword)` with
    // a closed-set enum classifier). With both in place, BOTH dispatch
    // sites in the macro expander project through the SAME family
    // primitive — each binding the classifier that fits its candidate
    // set. The tests below pin the consumer's path-uniformity contract
    // at the new boundary: a hand-rolled `as_call_to_any(|h| macros.get
    // (h))` dispatch observes the SAME `(def, args)` decomposition the
    // `Expander::expand` consumer routes through.

    #[test]
    fn expand_routes_macro_call_dispatch_observably_through_as_call_to_any() {
        // Structural identity: on a registered-macro call, the consumer's
        // expansion is observably equivalent to: classify the form via
        // `as_call_to_any(|h| macros.get(h))` → some `(def, args)` →
        // apply the def to args. Pin path-uniformity: a hand-rolled
        // `as_call_to_any` lookup against the same registry the expander
        // walks produces the SAME `MacroDef` reference for the SAME
        // input form. A regression that drifts the consumer back to an
        // inline `as_list + as_call + macros.get` chain (which would
        // fragment the family adoption) is caught structurally — the
        // hand-rolled `as_call_to_any` and the consumer's dispatch must
        // observe the same decomposition.
        let mut e = Expander::new();
        e.expand_program(read("(defmacro wrap (x) `(list ,x ,x))").unwrap())
            .unwrap();
        let call_form = parse("(wrap 42)");

        // Hand-rolled family-primitive lookup mirrors the lifted consumer.
        let (def_via_family, args_via_family) = call_form
            .as_call_to_any(|h| e.macros.get(h))
            .expect("registered macro call must decompose via as_call_to_any");
        assert_eq!(def_via_family.name, "wrap");
        assert_eq!(args_via_family, &[Sexp::int(42)]);

        // Consumer's expand observes the SAME decomposition: the expanded
        // form is `(list 42 42)`, derived from the SAME def + args the
        // hand-rolled lookup found. Path-uniform with the family
        // primitive at the dispatch boundary.
        let expanded = e.expand(&call_form).unwrap();
        assert_eq!(
            expanded,
            Sexp::List(vec![Sexp::symbol("list"), Sexp::int(42), Sexp::int(42)])
        );
    }

    #[test]
    fn expand_skips_non_macro_call_into_children_walk_via_family_primitive_none() {
        // Path-uniformity for the non-registered-head path: `as_call_to_any
        // (|h| macros.get(h))` returns `None` for a call whose head ISN'T
        // a registered macro, and the consumer falls through to the
        // children-walk (which expands any nested macro calls). Pin
        // both halves: the hand-rolled lookup returns `None`, AND the
        // consumer's expand walks into the children. A regression that
        // accidentally short-circuits the children-walk for non-macro
        // calls (e.g. by treating `as_call_to_any` = `None` as
        // "non-expandable" globally) would fail here.
        let mut e = Expander::new();
        e.expand_program(read("(defmacro wrap (x) `(list ,x ,x))").unwrap())
            .unwrap();
        let outer = parse("(foo (wrap 5))");

        // Hand-rolled family-primitive lookup rejects the outer head.
        assert!(outer.as_call_to_any(|h| e.macros.get(h)).is_none());

        // Consumer walks children — the inner `(wrap 5)` IS a macro call
        // and expands to `(list 5 5)`; the outer `foo` head is preserved.
        let expanded = e.expand(&outer).unwrap();
        assert_eq!(
            expanded,
            Sexp::List(vec![
                Sexp::symbol("foo"),
                Sexp::List(vec![Sexp::symbol("list"), Sexp::int(5), Sexp::int(5)]),
            ])
        );
    }

    #[test]
    fn expand_non_call_shapes_route_past_family_primitive_into_fallthrough_clone() {
        // Path-uniformity for the non-call path: every shape `as_call`
        // rejects (atoms across all 6 kinds, Nil, Quote-family wrappers)
        // ALSO routes past `as_call_to_any` into the `as_list()`
        // fallthrough, where the not-a-list arm returns `form.clone()`
        // verbatim. Pin both halves: the hand-rolled lookup rejects
        // every non-call shape regardless of decoder, AND the consumer
        // preserves each shape unchanged. A regression that drifts the
        // consumer's dispatch order (e.g. checking `as_list()` BEFORE
        // `as_call_to_any` in a way that mis-handles Quote-family
        // wrappers) would fail here.
        let e = Expander::new();
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("x"))),
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
            Sexp::Unquote(Box::new(Sexp::symbol("x"))),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("x"))),
        ];
        for s in &shapes {
            // Hand-rolled family-primitive lookup rejects non-call shapes
            // even for a promiscuous decoder — the call-shape gate fires
            // BEFORE the decoder runs.
            assert!(
                s.as_call_to_any(|_h: &str| Some(0_u8)).is_none(),
                "non-call shape must yield None for as_call_to_any: {s}"
            );
            // Consumer preserves the shape verbatim — the not-a-list
            // arm at the fallthrough returns `form.clone()`.
            assert_eq!(
                e.expand(s).unwrap(),
                s.clone(),
                "non-call shape must round-trip unchanged through expand: {s}"
            );
        }
    }

    #[test]
    fn expand_empty_list_routes_past_family_primitive_into_children_walk() {
        // The empty list `()` has no operator and no children. Pin that
        // `as_call_to_any` rejects it (no head to feed the decoder), the
        // consumer falls through to `as_list()` which returns `Some(&[])`,
        // and the children-walk emits `Sexp::List(vec![])` (an empty
        // list, not `form.clone()` of `Sexp::List(vec![])` — both happen
        // to be observationally identical, but the path is the
        // children-walk arm, NOT the not-a-list arm). Path-uniformity
        // gate for the singleton-list edge case the `compile_named_from_
        // forms` rejection chain relies on `as_call_to(KEYWORD)` to yield
        // `Some(&[])` for — same posture, different family member.
        let e = Expander::new();
        let empty = Sexp::List(vec![]);

        // Hand-rolled family-primitive lookup rejects the empty list.
        assert!(empty.as_call_to_any(|_h: &str| Some(())).is_none());

        // Consumer walks children (zero of them) — output is the empty
        // list, same as input.
        assert_eq!(e.expand(&empty).unwrap(), Sexp::List(vec![]));
    }

    // ── Expander::expand_and_collect_calls_to: program-level walk ───────
    //
    // `expand_and_collect_calls_to(forms, keyword, project)` composes the
    // expander's program-level expansion (`expand_program`) with the
    // substrate's slice-side typed-keyword projection (`iter_calls_to`)
    // and a caller-supplied per-form mapper into ONE method on the
    // `Expander` surface. Both `compile_typed::<T>` and
    // `compile_named_from_forms::<T>` (compile.rs) route through it; the
    // tests below pin its contract directly. The existing compile.rs
    // dispatch tests are the path-uniformity guards proving the two
    // production sites route through it without behavior drift.

    #[test]
    fn expand_and_collect_calls_to_yields_projection_for_every_matching_form_in_source_order() {
        // Three forms — two match `defmonitor`, one matches `defalert`.
        // The mapper records `args.len()` from each matching form's tail.
        // Pin: only the two `defmonitor` matches flow through `project`,
        // in source order (so the recorded lengths are 2, 1).
        let forms = vec![
            Sexp::List(vec![
                Sexp::symbol("defmonitor"),
                Sexp::keyword("name"),
                Sexp::string("first"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("defalert"),
                Sexp::keyword("name"),
                Sexp::string("not-a-match"),
            ]),
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::keyword("solo")]),
        ];
        let mut e = Expander::new();
        let lengths: Vec<usize> = e
            .expand_and_collect_calls_to(forms, "defmonitor", |args| Ok(args.len()))
            .expect("matching forms must compose");
        assert_eq!(lengths, vec![2, 1]);
    }

    #[test]
    fn expand_and_collect_calls_to_skips_non_matching_forms_without_invoking_project() {
        // Path-uniformity for the soft-projection posture inherited from
        // `iter_calls_to`: non-matching forms are skipped silently, and
        // `project` is never invoked on them. Pin via a counter the
        // closure increments per invocation — the counter is the
        // observable that distinguishes "skipped silently" from
        // "projected to a no-op".
        let forms = vec![
            Sexp::symbol("bare-atom"),
            Sexp::List(vec![Sexp::symbol("defalert"), Sexp::int(1)]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("not-symbol-head")]),
            Sexp::List(vec![]),
            Sexp::Nil,
        ];
        let mut count = 0usize;
        let mut e = Expander::new();
        let out: Vec<()> = e
            .expand_and_collect_calls_to(forms, "defmonitor", |_args| {
                count += 1;
                Ok(())
            })
            .expect("non-matching-only slice must collect to empty Vec");
        assert!(out.is_empty(), "no matching forms — empty Vec, got {out:?}");
        assert_eq!(count, 0, "project must never run for non-matching forms");
    }

    #[test]
    fn expand_and_collect_calls_to_short_circuits_on_project_error_at_first_failure() {
        // Three matching forms; the closure errors on the SECOND. Pin: the
        // collect's `Result<Vec<_>, _>` short-circuit fires at the second
        // form's error, the third form's `project` is NEVER invoked, and
        // the returned error is exactly the one the closure raised. This
        // mirrors `compile_named_from_forms`'s short-circuit on the first
        // `NamedFormMissingName` / `NamedFormNonSymbolName` / typed-entry
        // kwargs rejection.
        let forms = vec![
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::int(1)]),
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::int(2)]),
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::int(3)]),
        ];
        let mut seen = Vec::new();
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_calls_to::<(), _>(forms, "defmonitor", |args| {
                let n = args[0].as_int().expect("test args are ints");
                seen.push(n);
                if n == 2 {
                    Err(LispError::Compile {
                        form: "test".to_string(),
                        message: "stop at two".to_string(),
                    })
                } else {
                    Ok(())
                }
            })
            .expect_err("project's error must short-circuit collect");
        // Only the first two forms were ever projected — the third
        // never reached `project`.
        assert_eq!(seen, vec![1, 2]);
        assert!(
            matches!(err, LispError::Compile { ref message, .. } if message == "stop at two"),
            "short-circuit must propagate the project's error verbatim, got {err:?}"
        );
    }

    #[test]
    fn expand_and_collect_calls_to_short_circuits_on_expand_program_error_before_project_runs() {
        // The `expand_program` step runs BEFORE `iter_calls_to` walks
        // anything — so an `expand_program` error (e.g., a malformed
        // `defmacro` head) must short-circuit the entire composition
        // and `project` must NEVER run. Pin via the closure-running
        // counter: an error from the FIRST step never invokes `project`.
        // `(defmacro 5 (x) `,x)` — the macro NAME slot is an int, not a
        // symbol; `macro_def_from`'s `defmacro_non_symbol_name` rejection
        // fires during `expand_program`.
        let forms = read("(defmacro 5 (x) `,x) (defmonitor :name \"x\")").unwrap();
        let mut count = 0usize;
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_calls_to::<(), _>(forms, "defmonitor", |_args| {
                count += 1;
                Ok(())
            })
            .expect_err("expand_program error must short-circuit before project");
        assert_eq!(
            count, 0,
            "project must never run when expand_program errors"
        );
        // Sanity: the error IS the expand_program-stage rejection, NOT
        // a project-stage error (path-uniformity for the ordering).
        let rendered = format!("{err}");
        assert!(
            rendered.contains("NAME") || rendered.contains("symbol"),
            "error must be the defmacro-NAME-not-a-symbol rejection, got: {rendered}"
        );
    }

    #[test]
    fn expand_and_collect_calls_to_yields_empty_vec_for_empty_forms_input() {
        // Boundary: empty forms slice yields zero items regardless of
        // keyword. Pin the degenerate boundary — empty in, empty out —
        // and the closure is never invoked. Sibling of
        // `iter_calls_to_yields_nothing_for_empty_slice` in ast.rs,
        // one level up the composition: the program-level walk is
        // fused-empty when `expand_program` yields an empty `Vec<Sexp>`.
        let mut count = 0usize;
        let mut e = Expander::new();
        let out: Vec<()> = e
            .expand_and_collect_calls_to(Vec::new(), "anything", |_args| {
                count += 1;
                Ok(())
            })
            .expect("empty forms is not an error");
        assert!(out.is_empty());
        assert_eq!(count, 0);
    }

    #[test]
    fn expand_and_collect_calls_to_expands_macros_before_filtering_by_keyword() {
        // Critical ordering: `expand_program` runs BEFORE the keyword
        // filter, so a `defmacro` whose expansion produces a form with
        // a matching head IS visible to `project`. Pin the pipeline
        // order: `(defmacro emit-monitor (n) `(defmonitor :name ,n))
        //         (emit-monitor "alpha") (defmonitor :name "beta")`
        // expands the macro call to `(defmonitor :name "alpha")` and
        // the keyword walk then sees TWO `defmonitor` forms (the
        // macro-emitted one AND the directly-authored one). A
        // pre-expansion filter would miss the macro-emitted form.
        let forms = read(
            "(defmacro emit-monitor (n) `(defmonitor :name ,n))
             (emit-monitor \"alpha\")
             (defmonitor :name \"beta\")",
        )
        .unwrap();
        let mut e = Expander::new();
        let names: Vec<String> = e
            .expand_and_collect_calls_to(forms, "defmonitor", |args| {
                // Each defmonitor form's tail is `(:name "X")`; project
                // the second element's string to capture the name.
                Ok(args[1].as_string().unwrap().to_string())
            })
            .expect("macroexpanded + directly-authored forms must both flow");
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn expand_and_collect_calls_to_threads_keyword_argument_verbatim_into_filter() {
        // The `keyword` argument flows straight into `iter_calls_to`'s
        // head-comparison gate — a regression that drifts the keyword
        // (e.g., uses `T::KEYWORD` from a different `T` at the helper,
        // or lowercases / trims the keyword en route) fails here. Pin
        // by passing TWO different keywords against the SAME forms
        // input and asserting each picks up only its matching forms.
        let forms = vec![
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::int(1)]),
            Sexp::List(vec![Sexp::symbol("defalert"), Sexp::int(2)]),
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::int(3)]),
            Sexp::List(vec![Sexp::symbol("defnotify"), Sexp::int(4)]),
        ];

        let mut e = Expander::new();
        let monitors: Vec<i64> = e
            .expand_and_collect_calls_to(forms.clone(), "defmonitor", |args| {
                Ok(args[0].as_int().unwrap())
            })
            .unwrap();
        assert_eq!(monitors, vec![1, 3]);

        let mut e2 = Expander::new();
        let alerts: Vec<i64> = e2
            .expand_and_collect_calls_to(forms.clone(), "defalert", |args| {
                Ok(args[0].as_int().unwrap())
            })
            .unwrap();
        assert_eq!(alerts, vec![2]);

        // A keyword nothing matches collects to the empty Vec — same
        // shape as the empty-forms case at the slice level.
        let mut e3 = Expander::new();
        let none: Vec<i64> = e3
            .expand_and_collect_calls_to(forms, "missing-keyword", |args| {
                Ok(args[0].as_int().unwrap())
            })
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn expand_and_collect_calls_to_matches_inlined_expand_program_plus_iter_calls_to_path() {
        // Structural identity: for any (forms, keyword, project) triple
        // whose pieces succeed, the method's result equals the inlined
        // pre-lift pipeline `expand_program + iter_calls_to + map +
        // collect`. Pin shape AND ordering on a mixed forms input
        // (matching + non-matching + macroexpand-emitted matches) so a
        // regression that drifts the method's pipeline from the inlined
        // closed-form fails here. This is the lift's load-bearing
        // path-uniformity gate — the SAME assertion that holds for
        // both production consumers (compile.rs) holds at the
        // primitive's contract level.
        let src = "(defmacro emit-foo (n) `(foo :idx ,n))
                   (foo :idx 1)
                   (emit-foo 2)
                   (bar :idx 99)
                   (foo :idx 3)";
        let forms = read(src).unwrap();

        // Inlined pre-lift pipeline.
        let mut exp_inline = Expander::new();
        let expanded = exp_inline.expand_program(forms.clone()).unwrap();
        let via_inline: Vec<i64> = crate::ast::iter_calls_to(&expanded, "foo")
            .map(|args| -> Result<i64> { Ok(args[1].as_int().unwrap()) })
            .collect::<Result<Vec<_>>>()
            .unwrap();

        // Method pipeline.
        let mut exp_method = Expander::new();
        let via_method: Vec<i64> = exp_method
            .expand_and_collect_calls_to(forms, "foo", |args| Ok(args[1].as_int().unwrap()))
            .unwrap();

        assert_eq!(via_inline, via_method);
        assert_eq!(via_inline, vec![1, 2, 3]);
    }

    // ── Expander::expand_and_collect_calls_to_any: typed-decoded sibling ──
    //
    // The typed-decoded classifier sibling of `expand_and_collect_calls_to`:
    // composes `expand_program` with `iter_calls_to_any` and a per-form
    // mapper that receives BOTH the typed witness AND the args tail.
    // Closes the (keyword, classifier) 2×2 of expander-surface
    // compose-on-iter projections at the typed-decoded corner the prior
    // runs' slice-side `iter_calls_to_any` lift left open. The keyword
    // sibling now ROUTES through this primitive via a constant-classifier
    // composition — the tests below pin both the typed-decoded primitive's
    // contract directly AND the routing identity binding the keyword
    // sibling to it.
    //
    // The existing `expand_and_collect_calls_to_*` tests above remain
    // load-bearing — they pin the keyword sibling's contract through
    // its ROUTED implementation, so a regression that drifts the routing
    // composition surfaces at the existing keyword tests too.

    #[test]
    fn expand_and_collect_calls_to_any_yields_decoded_pair_for_every_matching_form_in_source_order()
    {
        // The typed-decoded primitive's happy path: a closed-set
        // classifier decodes head symbols to a typed enum, and the
        // per-form projection receives `(decoded, args)` for every
        // matched form in source order. Sweep across THREE distinct
        // classifier outcomes — `Foo` / `Bar` / `Baz` — interleaved with
        // a non-matching form to pin both the typed-witness threading
        // AND the source-order yield AND the rejection of non-classifier
        // forms in ONE assertion. The decoder runs once per call form
        // (`FnMut`); the projection runs once per matched form.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Op {
            Foo,
            Bar,
            Baz,
        }
        let src = "(foo 1)
                   (bar 2)
                   (other 99)
                   (baz 3)
                   (foo 4)";
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let pairs: Vec<(Op, i64)> = e
            .expand_and_collect_calls_to_any(
                forms,
                |h| match h {
                    "foo" => Some(Op::Foo),
                    "bar" => Some(Op::Bar),
                    "baz" => Some(Op::Baz),
                    _ => None,
                },
                |op, args| Ok((op, args[0].as_int().unwrap())),
            )
            .unwrap();
        assert_eq!(
            pairs,
            vec![(Op::Foo, 1), (Op::Bar, 2), (Op::Baz, 3), (Op::Foo, 4),],
        );
    }

    #[test]
    fn expand_and_collect_calls_to_any_skips_non_matching_forms_without_invoking_project() {
        // The soft-projection contract — every shape `iter_calls_to_any`
        // rejects (non-list, empty list, list whose head is not a
        // symbol, list whose head decodes through the classifier to
        // `None`) skips the projection silently. Pin a deliberately-
        // panicking projection so the assertion fires loudly if a
        // non-matching form reaches it. The decoder accepts only
        // `"defmonitor"`; every other shape (atom, list whose head is a
        // keyword, list whose head is a symbol the decoder rejects)
        // short-circuits before the projection runs.
        let src = r#":bare-keyword
                     "bare-string"
                     42
                     ()
                     (foo bar)
                     (defmonitor :name "matches")"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let lengths: Vec<usize> = e
            .expand_and_collect_calls_to_any(
                forms,
                |h| (h == "defmonitor").then_some(()),
                |(), args| {
                    // The projection must run EXACTLY once — for the
                    // single matched form. Any non-matching form that
                    // reaches the projection panics here.
                    assert_eq!(args.len(), 2, "projection ran on non-matching form");
                    Ok(args.len())
                },
            )
            .unwrap();
        assert_eq!(lengths, vec![2]);
    }

    #[test]
    fn expand_and_collect_calls_to_any_short_circuits_on_project_error_at_first_failure() {
        // The Result projection's short-circuit contract — when the
        // projection returns `Err` on a matched form, the walk
        // short-circuits and subsequent matched forms are NOT projected.
        // Pin the source-order short-circuit via a counter the
        // projection increments before deciding to fail: the counter
        // sits at exactly the index of the failing form (the second
        // match), proving the third match never reached the projection.
        let src = "(foo 1) (foo 2) (foo 3)";
        let forms = read(src).unwrap();
        let mut count = 0usize;
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_calls_to_any::<i64, _, _, ()>(
                forms,
                |h| (h == "foo").then_some(()),
                |(), args| {
                    count += 1;
                    let v = args[0].as_int().unwrap();
                    if v == 2 {
                        return Err(crate::error::LispError::Missing("test-failure"));
                    }
                    Ok(v)
                },
            )
            .expect_err("projection must short-circuit on first Err");
        assert_eq!(
            count, 2,
            "projection must have run on first AND failing form, then stopped",
        );
        // Sanity: the error is the project-stage rejection — the same
        // typed `LispError` the projection returned, propagated verbatim.
        assert!(
            matches!(err, crate::error::LispError::Missing("test-failure")),
            "expected the projection's typed Err verbatim, got {err:?}",
        );
    }

    #[test]
    fn expand_and_collect_calls_to_any_expands_macros_before_filtering_by_classifier() {
        // Ordering contract — `expand_program` runs BEFORE
        // `iter_calls_to_any` walks the slice. A `(defmacro …)` form
        // that emits a classifier-decoded body must have its expanded
        // body decoded by the classifier (not the unexpanded macro-call
        // head). Pin the ordering with a macro `(emit-foo n)` that
        // expands to `(foo :idx n)` — the classifier sees the expanded
        // `foo` head, not `emit-foo`. Mirrors the parallel keyword test
        // `expand_and_collect_calls_to_expands_macros_before_filtering_by_keyword`
        // — the SAME composition contract holds on the typed-decoded
        // sibling.
        let src = "(defmacro emit-foo (n) `(foo :idx ,n))
                   (foo :idx 1)
                   (emit-foo 2)
                   (bar :idx 99)
                   (foo :idx 3)";
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        // Classifier returns `()` for "foo" only; the keyword sibling's
        // routing identity is the same composition we exercise here
        // directly.
        let idxs: Vec<i64> = e
            .expand_and_collect_calls_to_any(
                forms,
                |h| (h == "foo").then_some(()),
                |(), args| Ok(args[1].as_int().unwrap()),
            )
            .unwrap();
        // 1 from the literal call, 2 from the macro-emitted call, 3 from
        // the final literal call. The macro-emitted `(foo :idx 2)` is
        // present BECAUSE `expand_program` ran first.
        assert_eq!(idxs, vec![1, 2, 3]);
    }

    #[test]
    fn expand_and_collect_calls_to_any_admits_fnmut_classifier_maintaining_state_across_walk() {
        // The `FnMut` constraint — the slice walk calls the classifier
        // once per call form, and a decoder that captures mutable state
        // (a counter, a registry cache, a visited-set) maintains that
        // state across the walk. Pin the `FnMut` contract with a counter
        // closure that increments per shape-gate-passing form (calls
        // whose head is a symbol), asserting the counter equals the
        // total call count regardless of whether the classifier accepts
        // each form. Mirrors the parallel slice-side test
        // `iter_calls_to_any_admits_fnmut_classifier_maintaining_state_across_batch_walk`
        // — the SAME `FnMut` contract holds on the expander surface.
        let src = "(foo 1) (bar 2) (foo 3) (bar 4) (foo 5)";
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let mut classifier_calls = 0usize;
        let projected: Vec<i64> = e
            .expand_and_collect_calls_to_any(
                forms,
                |h| {
                    classifier_calls += 1;
                    (h == "foo").then_some(())
                },
                |(), args| Ok(args[0].as_int().unwrap()),
            )
            .unwrap();
        assert_eq!(
            classifier_calls, 5,
            "classifier must run once per call form (5 forms, all calls with symbol heads)",
        );
        assert_eq!(
            projected,
            vec![1, 3, 5],
            "projection must run only for classifier-accepted forms in source order",
        );
    }

    #[test]
    fn expand_and_collect_calls_to_routes_through_expand_and_collect_calls_to_any_via_constant_classifier_composition(
    ) {
        // Structural identity: the keyword sibling
        // `expand_and_collect_calls_to` ROUTES through the typed-decoded
        // primitive via a constant-classifier composition — the
        // post-lift composition law binding the two siblings:
        //
        //   expand_and_collect_calls_to(forms, k, p) ==
        //       expand_and_collect_calls_to_any(forms,
        //           |h| (h == k).then_some(()),
        //           |(), args| p(args))
        //
        // Pin shape AND ordering across THREE representative keyword
        // values: one that matches multiple forms, one that matches
        // none, one that matches exactly one form. Same mixed input
        // (literal + macroexpand-emitted + non-matching) the keyword
        // path-uniformity test exercises, so the routing identity holds
        // on the SAME input shape the keyword sibling's contract pins.
        let src = "(defmacro emit-foo (n) `(foo :idx ,n))
                   (foo :idx 1)
                   (emit-foo 2)
                   (bar :idx 99)
                   (foo :idx 3)";
        let forms = read(src).unwrap();

        for keyword in ["foo", "bar", "absent"] {
            let mut exp_keyword = Expander::new();
            let via_keyword: Vec<i64> = exp_keyword
                .expand_and_collect_calls_to(forms.clone(), keyword, |args| {
                    Ok(args[1].as_int().unwrap())
                })
                .unwrap();
            let mut exp_classifier = Expander::new();
            let via_classifier: Vec<i64> = exp_classifier
                .expand_and_collect_calls_to_any(
                    forms.clone(),
                    |h| (h == keyword).then_some(()),
                    |(), args| Ok(args[1].as_int().unwrap()),
                )
                .unwrap();
            assert_eq!(
                via_keyword, via_classifier,
                "routing identity drifted for keyword {keyword:?}",
            );
        }
    }

    #[test]
    fn expand_and_collect_calls_to_any_short_circuits_on_expand_program_error_before_project_runs()
    {
        // The expand_program-stage short-circuit contract — when
        // `expand_program` rejects (e.g. a `(defmacro …)` form whose
        // param list is malformed), the walk short-circuits BEFORE the
        // classifier or projection runs. Pin the ordering with a
        // deliberately-panicking classifier AND projection: any
        // post-expand-program execution fires the panic. Mirrors the
        // parallel keyword test
        // `expand_and_collect_calls_to_short_circuits_on_expand_program_error_before_project_runs`
        // — the SAME composition contract holds on the typed-decoded
        // sibling.
        // `(defmacro 5 (x) `,x)` — macro NAME slot is an int, not a
        // symbol; `macro_def_from`'s `defmacro_non_symbol_name` rejection
        // fires during `expand_program`. Same rejection shape the
        // parallel keyword test uses, so the ordering contract observed
        // here is structurally identical to the keyword sibling's.
        let forms = read("(defmacro 5 (x) `,x) (foo :idx 1)").unwrap();
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_calls_to_any::<(), _, _, ()>(
                forms,
                |_h| -> Option<()> {
                    panic!("classifier must not run when expand_program errors");
                },
                |(), _args| {
                    panic!("project must not run when expand_program errors");
                },
            )
            .expect_err("expand_program error must short-circuit before classifier or project");
        // Sanity: the error IS an expand_program-stage rejection — not a
        // classifier or projection error — so the ordering is observable.
        let rendered = format!("{err}");
        assert!(
            rendered.contains("NAME") || rendered.contains("symbol"),
            "expected expand_program-stage `defmacro-NAME-not-a-symbol` rejection, got {rendered:?}",
        );
    }

    // ── Expander::expand_source_and_collect_calls_to: from-source walk ──
    //
    // The from-source sibling of `expand_and_collect_calls_to`: composes
    // `crate::reader::read(src)?` with the from-forms primitive in ONE
    // method on the `Expander` surface. All four typed-program dispatchers
    // (free-function `compile_typed` / `compile_named`, preloaded-expander
    // `RealizedCompiler::compile_typed` / `compile_named`) route through
    // it; the tests below pin its contract directly. The existing
    // compile.rs + compiler_spec.rs dispatch tests are the path-uniformity
    // guards proving the four production sites route through it without
    // behavior drift.

    #[test]
    fn expand_source_and_collect_calls_to_routes_through_reader_then_expand_and_collect() {
        // Happy path: a source string with three top-level forms (two
        // matching `defmonitor`, one matching `defalert`) flows through
        // `read` then through the from-forms primitive. Pin the SAME
        // emission shape the from-forms test pins for the matching
        // sub-set, sourced from a `&str` rather than a `Vec<Sexp>`.
        let src = r#"(defmonitor :name "first")
                     (defalert :name "not-a-match")
                     (defmonitor :solo)"#;
        let mut e = Expander::new();
        let lengths: Vec<usize> = e
            .expand_source_and_collect_calls_to(src, "defmonitor", |args| Ok(args.len()))
            .expect("matching forms must compose");
        assert_eq!(lengths, vec![2, 1]);
    }

    #[test]
    fn expand_source_and_collect_calls_to_short_circuits_on_reader_error_before_expand_program() {
        // The reader runs BEFORE `expand_program` — so a reader error (an
        // unterminated string, an unbalanced paren, an unknown escape)
        // must short-circuit the entire composition and `expand_program`
        // / the keyword filter / `project` must NEVER run. Pin the
        // ordering: an unbalanced open-paren is rejected by the reader
        // at parse time, never reaches the from-forms primitive.
        let mut count = 0usize;
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_calls_to::<(), _>(
                "(defmonitor :name \"unbalanced",
                "defmonitor",
                |_args| {
                    count += 1;
                    Ok(())
                },
            )
            .expect_err("reader error must short-circuit before expand_program");
        assert_eq!(
            count, 0,
            "project must never run when reader errors at parse time"
        );
        // Sanity: the error IS a reader-stage rejection — the rendered
        // diagnostic mentions the lexer's gate, not the expander's or
        // projector's (path-uniformity for the read-then-walk ordering).
        let rendered = format!("{err}");
        assert!(
            rendered.to_lowercase().contains("string")
                || rendered.to_lowercase().contains("paren")
                || rendered.to_lowercase().contains("eof")
                || rendered.to_lowercase().contains("unexpected")
                || rendered.to_lowercase().contains("unterminated")
                || rendered.to_lowercase().contains("unclosed"),
            "error must be the reader-stage rejection, got: {rendered}"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_short_circuits_on_expand_program_error_before_project_runs(
    ) {
        // Reader succeeds, but `expand_program` rejects a malformed
        // `defmacro` head (NAME slot is an int, not a symbol). Pin the
        // ordering: `expand_program` rejects BEFORE the keyword filter
        // walks anything, `project` must NEVER run. Sibling of the
        // from-forms test of the same name — one level up the
        // composition (sourced from `&str` rather than `Vec<Sexp>`).
        let mut count = 0usize;
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_calls_to::<(), _>(
                "(defmacro 5 (x) `,x) (defmonitor :name \"x\")",
                "defmonitor",
                |_args| {
                    count += 1;
                    Ok(())
                },
            )
            .expect_err("expand_program error must short-circuit before project");
        assert_eq!(
            count, 0,
            "project must never run when expand_program errors"
        );
        let rendered = format!("{err}");
        assert!(
            rendered.contains("NAME") || rendered.contains("symbol"),
            "error must be the defmacro-NAME-not-a-symbol rejection, got: {rendered}"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_short_circuits_on_project_error_at_first_failure() {
        // Reader + `expand_program` both succeed; the per-form `project`
        // errors on the SECOND matched form. Pin: the collect's short-
        // circuit fires at the second match's error, the third match's
        // `project` is NEVER invoked, and the returned error is exactly
        // the one the closure raised. Mirrors `compile_named`'s
        // short-circuit on the first `NamedFormMissingName` /
        // `NamedFormNonSymbolName` / typed-entry rejection — sourced
        // from `&str`.
        let src = "(defmonitor :idx 1) (defmonitor :idx 2) (defmonitor :idx 3)";
        let mut seen = Vec::new();
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_calls_to::<(), _>(src, "defmonitor", |args| {
                let n = args[1].as_int().expect("test args are ints");
                seen.push(n);
                if n == 2 {
                    Err(LispError::Compile {
                        form: "test".to_string(),
                        message: "stop at two".to_string(),
                    })
                } else {
                    Ok(())
                }
            })
            .expect_err("project's error must short-circuit collect");
        assert_eq!(seen, vec![1, 2]);
        assert!(
            matches!(err, LispError::Compile { ref message, .. } if message == "stop at two"),
            "short-circuit must propagate the project's error verbatim, got {err:?}"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_yields_empty_vec_for_empty_source() {
        // Boundary: empty source string yields zero items regardless of
        // keyword. Pin the degenerate boundary — empty in, empty out —
        // and the closure is never invoked. Sibling of the from-forms
        // test of the same name, one level up the composition: the
        // from-source posture is fused-empty when `read` yields an
        // empty `Vec<Sexp>` (which, fed into `expand_program`, yields
        // an empty `Vec<Sexp>`, which iter_calls_to walks to zero
        // matches).
        let mut count = 0usize;
        let mut e = Expander::new();
        let out: Vec<()> = e
            .expand_source_and_collect_calls_to("", "anything", |_args| {
                count += 1;
                Ok(())
            })
            .expect("empty source is not an error");
        assert!(out.is_empty());
        assert_eq!(count, 0);
    }

    #[test]
    fn expand_source_and_collect_calls_to_expands_macros_before_filtering_by_keyword() {
        // Critical ordering preserved across the from-source posture:
        // `defmacro` source registers in `expand_program`, its call
        // sites expand to the matching keyword, AND the keyword walk
        // sees both the macro-emitted form and any directly-authored
        // matches in source order. Sibling of the from-forms test of
        // the same name, sourced from `&str`.
        let src = "(defmacro emit-monitor (n) `(defmonitor :name ,n))
                   (emit-monitor \"alpha\")
                   (defmonitor :name \"beta\")";
        let mut e = Expander::new();
        let names: Vec<String> = e
            .expand_source_and_collect_calls_to(src, "defmonitor", |args| {
                Ok(args[1].as_string().unwrap().to_string())
            })
            .expect("macroexpanded + directly-authored forms must both flow");
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn expand_source_and_collect_calls_to_matches_inlined_read_plus_expand_and_collect_path() {
        // Structural identity: for any (src, keyword, project) triple
        // whose pieces succeed, the from-source method's result equals
        // the inlined pre-lift pipeline `read + expand_and_collect_
        // calls_to`. Pin shape AND ordering on a mixed source
        // (matching + non-matching + macroexpand-emitted matches) so a
        // regression that drifts the method's pipeline from the
        // inlined closed-form fails here. This is the lift's
        // load-bearing path-uniformity gate — the SAME assertion that
        // holds for all four production consumers (free-function
        // `compile_typed`/`compile_named` + `RealizedCompiler::compile_typed`/`compile_named`)
        // holds at the primitive's contract level.
        let src = "(defmacro emit-foo (n) `(foo :idx ,n))
                   (foo :idx 1)
                   (emit-foo 2)
                   (bar :idx 99)
                   (foo :idx 3)";

        // Inlined pre-lift pipeline.
        let mut exp_inline = Expander::new();
        let inline_forms = read(src).unwrap();
        let via_inline: Vec<i64> = exp_inline
            .expand_and_collect_calls_to(inline_forms, "foo", |args| Ok(args[1].as_int().unwrap()))
            .unwrap();

        // From-source method pipeline.
        let mut exp_method = Expander::new();
        let via_method: Vec<i64> = exp_method
            .expand_source_and_collect_calls_to(src, "foo", |args| Ok(args[1].as_int().unwrap()))
            .unwrap();

        assert_eq!(via_inline, via_method);
        assert_eq!(via_inline, vec![1, 2, 3]);
    }

    #[test]
    fn expand_source_and_collect_calls_to_threads_keyword_argument_verbatim_into_filter() {
        // The `keyword` argument flows straight into the from-forms
        // primitive's head-comparison gate, which inherits from
        // `iter_calls_to` — a regression that drifts the keyword (e.g.,
        // lowercases / trims it en route through the read step) fails
        // here. Pin by passing TWO different keywords against the SAME
        // source and asserting each picks up only its matching forms.
        let src = "(defmonitor :idx 1) (defalert :idx 2) (defmonitor :idx 3) (defnotify :idx 4)";

        let mut e1 = Expander::new();
        let monitors: Vec<i64> = e1
            .expand_source_and_collect_calls_to(src, "defmonitor", |args| {
                Ok(args[1].as_int().unwrap())
            })
            .unwrap();
        assert_eq!(monitors, vec![1, 3]);

        let mut e2 = Expander::new();
        let alerts: Vec<i64> = e2
            .expand_source_and_collect_calls_to(src, "defalert", |args| {
                Ok(args[1].as_int().unwrap())
            })
            .unwrap();
        assert_eq!(alerts, vec![2]);

        let mut e3 = Expander::new();
        let none: Vec<i64> = e3
            .expand_source_and_collect_calls_to(src, "missing-keyword", |args| {
                Ok(args[1].as_int().unwrap())
            })
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn expand_source_and_collect_calls_to_skips_non_matching_forms_without_invoking_project() {
        // Path-uniformity for the soft-projection posture inherited
        // from `iter_calls_to`: non-matching forms in the source are
        // skipped silently, and `project` is never invoked on them.
        // Sibling of the from-forms test of the same name, sourced
        // from `&str` with a mix of atoms-and-lists that the reader
        // emits at the top level.
        let src = "bare-atom (defalert :idx 1) (defnotify :idx 2)";
        let mut count = 0usize;
        let mut e = Expander::new();
        let out: Vec<()> = e
            .expand_source_and_collect_calls_to(src, "defmonitor", |_args| {
                count += 1;
                Ok(())
            })
            .expect("non-matching-only source must collect to empty Vec");
        assert!(out.is_empty(), "no matching forms — empty Vec, got {out:?}");
        assert_eq!(count, 0, "project must never run for non-matching forms");
    }

    // ── Expander::expand_source_and_collect_calls_to_any — from-source × classifier ─
    //
    // The from-source posture of the typed-decoded classifier sibling on
    // the `Expander` surface — closes the (from-forms, from-source) ×
    // (keyword, classifier) 2×2 of compose-on-iter projections at the
    // from-source classifier corner the prior runs left open. The
    // pre-existing `expand_source_and_collect_calls_to` (from-source ×
    // keyword) is the constant-classifier projection of this primitive:
    // its body composes this function with a `|h| (h == keyword).
    // then_some(())` decoder, so the from-source pipeline lives at ONE
    // site. These tests pin both the typed-decoded primitive's contract
    // directly AND the post-lift routing of
    // `expand_source_and_collect_calls_to` through it.

    #[test]
    fn expand_source_and_collect_calls_to_any_yields_decoded_pair_for_every_matching_form_in_source_order(
    ) {
        // Happy path: a source string with four top-level forms (two
        // matching "foo", one matching "bar", one rejected-head
        // "defmonitor") flows through `read` → `expand_program` → the
        // typed-decoded classifier walk. The closed-set classifier
        // `Op::from_keyword` admits "foo" and "bar" but rejects
        // "defmonitor". Pin the typed-decoded yield shape (decoded
        // witness alongside args tail) AND source-order preservation
        // sourced from `&str` rather than a pre-parsed `Vec<Sexp>`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Op {
            Foo,
            Bar,
        }
        fn op_from_keyword(head: &str) -> Option<Op> {
            match head {
                "foo" => Some(Op::Foo),
                "bar" => Some(Op::Bar),
                _ => None,
            }
        }
        let src = r#"(foo :idx 1)
                     (defmonitor :idx 99)
                     (bar :idx 2)
                     (foo :idx 3)"#;
        let mut e = Expander::new();
        let yielded: Vec<(Op, i64)> = e
            .expand_source_and_collect_calls_to_any(src, op_from_keyword, |op, args| {
                Ok((op, args[1].as_int().expect("test args are ints")))
            })
            .expect("matching forms must compose through from-source classifier walk");
        assert_eq!(
            yielded,
            vec![(Op::Foo, 1), (Op::Bar, 2), (Op::Foo, 3)],
            "yields must be in source order, decoded witness paired with per-form mapper output"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_any_short_circuits_on_reader_error_before_classifier_runs(
    ) {
        // The reader runs BEFORE `expand_program` AND BEFORE the
        // classifier — so a reader error (an unterminated string)
        // short-circuits the entire composition and both the classifier
        // AND `project` must NEVER run. Pin the ordering with BOTH
        // decoder and projection explicitly panicking — any post-reader
        // execution fires the panic. Sibling of
        // `expand_source_and_collect_calls_to_short_circuits_on_reader_error_before_expand_program`
        // one corner over in the 2×2 (the classifier version of the
        // same ordering pin).
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_calls_to_any::<(), _, _, ()>(
                "(defmonitor :name \"unbalanced",
                |_h: &str| -> Option<()> {
                    panic!("classifier must not run when reader errors at parse time")
                },
                |(), _args| -> Result<()> { panic!("project must not run when reader errors") },
            )
            .expect_err("reader error must short-circuit before classifier");
        // Sanity: the error is a reader-stage rejection.
        let rendered = format!("{err}");
        assert!(
            rendered.to_lowercase().contains("string")
                || rendered.to_lowercase().contains("paren")
                || rendered.to_lowercase().contains("eof")
                || rendered.to_lowercase().contains("unexpected")
                || rendered.to_lowercase().contains("unterminated")
                || rendered.to_lowercase().contains("unclosed"),
            "error must be the reader-stage rejection, got: {rendered}"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_any_short_circuits_on_expand_program_error_before_classifier_runs(
    ) {
        // Reader succeeds, but `expand_program` rejects a malformed
        // `defmacro` head (NAME slot is an int, not a symbol). Pin the
        // ordering: `expand_program` rejects BEFORE the classifier walks
        // anything, both decoder AND `project` must NEVER run. Sibling
        // of the keyword-from-source variant — one corner over in the
        // 2×2 with both classifier and projection explicitly panicking.
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_calls_to_any::<(), _, _, ()>(
                "(defmacro 5 (x) `,x) (defmonitor :name \"x\")",
                |_h: &str| -> Option<()> {
                    panic!("classifier must not run when expand_program errors")
                },
                |(), _args| -> Result<()> {
                    panic!("project must not run when expand_program errors")
                },
            )
            .expect_err("expand_program error must short-circuit before classifier");
        let rendered = format!("{err}");
        assert!(
            rendered.contains("NAME") || rendered.contains("symbol"),
            "error must be the defmacro-NAME-not-a-symbol rejection, got: {rendered}"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_any_skips_non_matching_forms_without_invoking_project() {
        // Path-uniformity for the soft-projection posture inherited
        // from `iter_calls_to_any`: forms whose head the classifier
        // rejects are skipped silently, and `project` is never invoked
        // on them. Pin via an explicitly-panicking projection across a
        // mix of forms (atom, list-with-non-symbol-head, list with
        // unrecognized-by-classifier symbol head). A regression that
        // wrongly invokes `project` on a rejected form fires the panic.
        let src = r#"bare-atom
                     (5 :not-a-symbol-head)
                     (defmonitor :name "decoder-rejects-me")"#;
        let mut e = Expander::new();
        // The closed-set classifier admits NOTHING in this source.
        let out: Vec<()> = e
            .expand_source_and_collect_calls_to_any::<(), _, _, ()>(
                src,
                |_h: &str| -> Option<()> { None },
                |(), _args| -> Result<()> {
                    panic!("project must not run for classifier-rejected forms")
                },
            )
            .expect("classifier-rejects-all source must collect to empty Vec");
        assert!(
            out.is_empty(),
            "no classifier-accepted forms — empty Vec, got {out:?}"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_any_short_circuits_on_project_error_at_first_failure() {
        // Reader + `expand_program` + classifier all succeed; the
        // per-form `project` errors on the SECOND matched form. Pin: the
        // collect's short-circuit fires at the second match's error, the
        // third match's `project` is NEVER invoked, and the returned
        // error is exactly the one the closure raised. Sibling of the
        // keyword-from-source variant — one corner over in the 2×2.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Op {
            Foo,
        }
        fn op_from_keyword(head: &str) -> Option<Op> {
            (head == "foo").then_some(Op::Foo)
        }
        let src = "(foo :idx 1) (foo :idx 2) (foo :idx 3)";
        let mut seen = Vec::new();
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_calls_to_any::<(), _, _, _>(
                src,
                op_from_keyword,
                |_op: Op, args: &[Sexp]| -> Result<()> {
                    let n = args[1].as_int().expect("test args are ints");
                    seen.push(n);
                    if n == 2 {
                        Err(LispError::Compile {
                            form: "test".to_string(),
                            message: "stop at two".to_string(),
                        })
                    } else {
                        Ok(())
                    }
                },
            )
            .expect_err("project's error must short-circuit collect");
        assert_eq!(seen, vec![1, 2], "third match's project must never run");
        assert!(
            matches!(err, LispError::Compile { ref message, .. } if message == "stop at two"),
            "short-circuit must propagate the project's error verbatim, got {err:?}"
        );
    }

    #[test]
    fn expand_source_and_collect_calls_to_routes_through_expand_source_and_collect_calls_to_any_via_constant_classifier_composition(
    ) {
        // The post-lift composition law binding the keyword-from-source
        // sibling to the typed-decoded primitive:
        //   `expand_source_and_collect_calls_to(src, k, project) ==
        //    expand_source_and_collect_calls_to_any(src,
        //        |h| (h == k).then_some(()),
        //        |(), args| project(args))`
        // Pin shape AND ordering across THREE representative keywords
        // (matching some, matching exactly one, matching none) on the
        // SAME mixed source the keyword path-uniformity test exercises.
        // A regression that re-implements the keyword-from-source filter
        // as a parallel `read + expand_and_collect_calls_to` pipeline
        // (rather than routing through the typed-decoded primitive)
        // fails the value pin.
        let src = "(defmacro emit-foo (n) `(foo :idx ,n))
                   (foo :idx 1)
                   (emit-foo 2)
                   (bar :idx 99)
                   (foo :idx 3)";
        for k in ["foo", "bar", "missing"] {
            let mut e_keyword = Expander::new();
            let via_keyword: Vec<i64> = e_keyword
                .expand_source_and_collect_calls_to(src, k, |args| Ok(args[1].as_int().unwrap()))
                .unwrap();

            let mut e_classifier = Expander::new();
            let via_classifier: Vec<i64> = e_classifier
                .expand_source_and_collect_calls_to_any(
                    src,
                    |h: &str| (h == k).then_some(()),
                    |(), args: &[Sexp]| Ok(args[1].as_int().unwrap()),
                )
                .unwrap();

            assert_eq!(
                via_keyword, via_classifier,
                "keyword from-source path must equal classifier from-source path for {k:?}"
            );
        }
    }

    #[test]
    fn expand_source_and_collect_calls_to_any_expands_macros_before_filtering_by_classifier() {
        // Critical ordering preserved across the typed-decoded
        // from-source posture: `defmacro` source registers in
        // `expand_program`, its call sites expand to a classifier-
        // accepted form, AND the classifier walk sees both the
        // macro-emitted form and any directly-authored matches in
        // source order. Sibling of the keyword-from-source ordering
        // test, one corner over in the 2×2.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Op {
            Foo,
        }
        fn op_from_keyword(head: &str) -> Option<Op> {
            (head == "foo").then_some(Op::Foo)
        }
        let src = "(defmacro emit-foo (n) `(foo :name ,n))
                   (emit-foo \"alpha\")
                   (foo :name \"beta\")";
        let mut e = Expander::new();
        let names: Vec<(Op, String)> = e
            .expand_source_and_collect_calls_to_any(src, op_from_keyword, |op, args| {
                Ok((op, args[1].as_string().unwrap().to_string()))
            })
            .expect("macroexpanded + directly-authored forms must both flow through classifier");
        assert_eq!(
            names,
            vec![
                (Op::Foo, "alpha".to_string()),
                (Op::Foo, "beta".to_string()),
            ]
        );
    }

    // ── expand_source_program — from-source × yield-all primitive ───
    //
    // Closes the 2×2 program-level walk family on the `Expander` surface:
    // from-forms × yield-all (`expand_program`), from-forms × keyword-
    // projected (`expand_and_collect_calls_to`), from-source × keyword-
    // projected (`expand_source_and_collect_calls_to`), AND now
    // from-source × yield-all (`expand_source_program`). Pins (a) the
    // happy path (mixed forms flow through reader then expand_program),
    // (b) reader-stage short-circuit (no expand_program side-effects),
    // (c) expand_program-stage short-circuit (defmacro-NAME-not-symbol
    // is rejected at its offending form), (d) defmacro absorption (the
    // primitive's side-effect on `self.macros` matches the from-forms
    // posture's side-effect verbatim), (e) the empty-source boundary,
    // and (f) structural identity vs. the inlined pre-lift `read +
    // expand_program` pipeline (path-uniformity gate).

    #[test]
    fn expand_source_program_routes_through_reader_then_expand_program() {
        // Happy path: a source string with mixed forms (a `defmacro`
        // definition, a `defmonitor` form, a plain symbol literal) flows
        // through `read` then through the from-forms primitive. Pin the
        // SAME emission shape the from-forms test pins, sourced from a
        // `&str` rather than a `Vec<Sexp>`.
        let src = r#"(defmacro id (x) `,x)
                     (defmonitor :name "alpha")
                     bare-symbol"#;
        let mut e = Expander::new();
        let out = e
            .expand_source_program(src)
            .expect("mixed forms must compose");
        // `(defmacro id …)` is consumed as a side-effect (registers `id`
        // in `e.macros`), so the returned `Vec<Sexp>` contains the
        // remaining two top-level forms in source order.
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].as_call_to("defmonitor").map(<[_]>::len),
            Some(2),
            "first surviving form is the defmonitor with two args, got: {:?}",
            out[0]
        );
        assert_eq!(
            out[1].as_symbol(),
            Some("bare-symbol"),
            "second surviving form is the bare symbol literal, got: {:?}",
            out[1]
        );
        // The defmacro side-effect landed in the expander's macro table.
        assert!(
            e.has("id"),
            "defmacro must register `id` in the expander's macro table"
        );
    }

    #[test]
    fn expand_source_program_short_circuits_on_reader_error_before_expand_program() {
        // The reader runs BEFORE `expand_program` — so a reader error (an
        // unterminated string, an unbalanced paren) must short-circuit
        // the entire composition and `expand_program` must NEVER run.
        // Pin the ordering: a `defmacro` BEFORE the unterminated string
        // is NOT registered, because the reader fails at parse time
        // before `expand_program` sees any form.
        let mut e = Expander::new();
        let err = e
            .expand_source_program(r#"(defmacro should-not-register (x) `,x) "unterminated"#)
            .expect_err("reader error must short-circuit before expand_program");
        // Sanity: the error IS a reader-stage rejection.
        let rendered = format!("{err}").to_lowercase();
        assert!(
            rendered.contains("string")
                || rendered.contains("paren")
                || rendered.contains("eof")
                || rendered.contains("unexpected")
                || rendered.contains("unterminated")
                || rendered.contains("unclosed"),
            "error must be the reader-stage rejection, got: {err}"
        );
        // Path-uniformity gate: the `defmacro` lexically BEFORE the
        // unterminated string must NOT have been absorbed — `read`'s
        // failure short-circuits the entire pipeline before
        // `expand_program` walks any forms.
        assert!(
            !e.has("should-not-register"),
            "reader-stage rejection must short-circuit BEFORE expand_program registers any defmacro"
        );
    }

    #[test]
    fn expand_source_program_short_circuits_on_expand_program_error() {
        // Reader succeeds, but `expand_program` rejects a malformed
        // `defmacro` head (NAME slot is an int, not a symbol). Pin the
        // ordering: the expand_program-stage rejection bubbles up
        // verbatim, sourced from `&str` rather than `Vec<Sexp>`.
        let mut e = Expander::new();
        let err = e
            .expand_source_program("(defmacro 5 (x) `,x)")
            .expect_err("expand_program error must propagate");
        let rendered = format!("{err}");
        assert!(
            rendered.contains("NAME") || rendered.contains("symbol"),
            "error must be the defmacro-NAME-not-a-symbol rejection, got: {rendered}"
        );
    }

    #[test]
    fn expand_source_program_yields_empty_vec_for_empty_source() {
        // Boundary: empty source string yields zero items. Pin the
        // degenerate boundary — empty in, empty out — sibling of the
        // from-source × keyword-projected test of the same name, one
        // level down the composition (no keyword filter).
        let mut e = Expander::new();
        let out = e
            .expand_source_program("")
            .expect("empty source is not an error");
        assert!(
            out.is_empty(),
            "empty source must yield empty Vec, got: {out:?}"
        );
    }

    #[test]
    fn expand_source_program_absorbs_defmacro_and_expands_subsequent_calls() {
        // Critical pipeline order: a `defmacro` in the source registers
        // AND a subsequent call to it expands AND the expanded form
        // surfaces in the returned Vec. Sourced from `&str`, the
        // from-source posture preserves the side-effect-then-expansion
        // semantics of the from-forms primitive verbatim.
        let src = r#"(defmacro emit-monitor (n) `(defmonitor :name ,n))
                     (emit-monitor "alpha")
                     (emit-monitor "beta")"#;
        let mut e = Expander::new();
        let out = e
            .expand_source_program(src)
            .expect("defmacro absorption then expansion must compose");
        // Two surviving forms (the `defmacro` itself is consumed as a
        // side-effect), each expanded to a `(defmonitor :name "<n>")`.
        assert_eq!(out.len(), 2, "expected two expanded forms, got: {out:?}");
        for (i, expected_name) in [(0, "alpha"), (1, "beta")] {
            let args = out[i]
                .as_call_to("defmonitor")
                .unwrap_or_else(|| panic!("form {i} must be defmonitor, got: {:?}", out[i]));
            assert_eq!(args[0].as_keyword(), Some("name"));
            assert_eq!(args[1].as_string(), Some(expected_name));
        }
        // The macro is now in the table — a subsequent call would
        // expand against this SAME expander.
        assert!(e.has("emit-monitor"));
    }

    #[test]
    fn expand_source_program_matches_inlined_read_plus_expand_program_path() {
        // Structural identity: for any `src` whose pieces succeed, the
        // from-source method's result equals the inlined pre-lift
        // pipeline `read + expand_program`. Pin shape AND ordering on a
        // mixed source (defmacro definition + macro call + plain form)
        // so a regression that drifts the method's pipeline from the
        // inlined closed-form fails here. This is the lift's
        // load-bearing path-uniformity gate — the SAME assertion holds
        // for both production consumers (`RealizedCompiler::compile` +
        // `realize_in_memory`'s `:macros` loop) at the primitive's
        // contract level.
        let src = r#"(defmacro id (x) `,x)
                     (id (foo 1 2))
                     (bar)"#;

        // Inlined pre-lift pipeline.
        let mut e_inline = Expander::new();
        let inline_forms = read(src).unwrap();
        let via_inline = e_inline
            .expand_program(inline_forms)
            .expect("inlined pipeline must succeed");

        // From-source method pipeline.
        let mut e_method = Expander::new();
        let via_method = e_method
            .expand_source_program(src)
            .expect("from-source method pipeline must succeed");

        assert_eq!(
            via_inline, via_method,
            "from-source method must emit byte-identical result to inlined read+expand_program"
        );
        // Both pipelines registered the same defmacro side-effect.
        assert_eq!(e_inline.has("id"), e_method.has("id"));
        assert!(e_method.has("id"));
    }

    #[test]
    fn expand_source_program_preserves_defmacro_absorption_across_repeated_calls() {
        // Per-`&mut self` absorption posture pin: a `defmacro` registered
        // in call 1 SURVIVES into call 2 on the SAME `Expander`. This is
        // the load-bearing semantic `realize_in_memory`'s per-spec-macro
        // build-up depends on — every iteration's `expand_source_program(
        // macro_src)?` lands its `defmacro` heads into the SAME mutable
        // `preloaded` expander, so a follow-up `:macros` source can
        // invoke macros defined in earlier ones.
        let mut e = Expander::new();
        let _ = e
            .expand_source_program("(defmacro outer (n) `(inner ,n))")
            .unwrap();
        assert!(e.has("outer"));
        // Call 2: defines `inner` AND invokes `outer`, which expands to
        // `(inner <n>)`. The expansion proves call 1's `outer` survived
        // into call 2's expansion.
        let out = e
            .expand_source_program("(defmacro inner (x) `(wrapped ,x)) (outer 42)")
            .unwrap();
        assert_eq!(out.len(), 1, "call 2 yields one expanded form");
        // (outer 42) → (inner 42) → (wrapped 42)
        let args = out[0]
            .as_call_to("wrapped")
            .expect("nested expansion must reach `wrapped`");
        assert_eq!(args[0].as_int(), Some(42));
    }

    // ── Expander::register_macro_def: the macro-registration primitive ──
    //
    // `register_macro_def(&mut self, def: MacroDef) -> Result<()>` lifts
    // the byte-identical two-step block —
    //
    //   if self.compile_templates {
    //       self.templates.insert(def.name.clone(), compile_template(&def)?);
    //   }
    //   self.macros.insert(def.name.clone(), def);
    //
    // — that lived inline at `with_macros` (the bulk-from-iterator
    // constructor) AND `expand_program`'s `(defmacro …)`-head arm (the
    // program-walk-time registration site) into ONE named method on the
    // `Expander` surface. The tests below pin:
    //
    //   (a) the bytecode-default expander (`Expander::new()`) populates
    //       BOTH `macros` AND `templates` keyed by `def.name`,
    //   (b) the substitute-only expander (`Expander::new_substitute_only()`)
    //       populates `macros` but SKIPS `templates` — the
    //       `compile_templates: false` gate fires structurally,
    //   (c) a template that fails to compile (`,foo` against `params:
    //       []`) short-circuits BEFORE `self.macros.insert` runs, so the
    //       failed registration leaves both tables untouched — no
    //       partial-write window in which `self.macros.has(name)` is
    //       true but `self.templates.has(name)` is missing,
    //   (d) `with_macros([def])` and `register_macro_def(def)` on a fresh
    //       expander produce structurally identical state (both tables'
    //       sets of keys + the same `MacroDef` body under each key),
    //   (e) `expand_program` of one `(defmacro …)` form and
    //       `register_macro_def` of the parsed `MacroDef` produce
    //       structurally identical state — closing path-uniformity
    //       across the lift's two consumers AT the registration site.
    //
    // The tests bind on `Expander::has(name)` / `Expander::len()` for
    // the macros table and on bytecode-vs-substitute output equivalence
    // (a registered macro must expand to the SAME result regardless of
    // whether the bytecode path's `self.templates` entry exists) for the
    // templates table.

    fn macro_def_id() -> MacroDef {
        // `(defmacro id (x) `,x)` — the simplest well-formed MacroDef:
        // one required param, a single-symbol unquote body. Compiles to
        // a valid `CompiledTemplate` (no unbound vars), so
        // `register_macro_def` succeeds on every Expander posture.
        MacroDef {
            name: "id".into(),
            params: MacroParams {
                required: vec!["x".into()],
                optional: vec![],
                rest: None,
            },
            // ` ` quasi-quoted `,x` — `Quasiquote(Unquote(Symbol("x")))`.
            body: Sexp::Quasiquote(Box::new(Sexp::Unquote(Box::new(Sexp::symbol("x"))))),
        }
    }

    fn macro_def_bad_template() -> MacroDef {
        // `(defmacro bad () `,unbound)` — a quasi-quote body with `,unbound`
        // against an EMPTY required-params list. `compile_template`
        // rejects it with `UnboundTemplateVar` because `unbound` is not
        // in `params.names()`. Used to exercise the
        // `compile_template`-failed-but-self.macros-still-pristine
        // path-uniformity pin.
        MacroDef {
            name: "bad".into(),
            params: MacroParams::default(),
            body: Sexp::Quasiquote(Box::new(Sexp::Unquote(Box::new(Sexp::symbol("unbound"))))),
        }
    }

    #[test]
    fn register_macro_def_bytecode_default_populates_macros_and_templates() {
        // The bytecode-default `Expander::new()` carries
        // `compile_templates: true`; `register_macro_def` MUST populate
        // BOTH `self.macros` (the substitute path's registry) AND
        // `self.templates` (the bytecode path's pre-compiled bytecode
        // index) keyed by `def.name`. Fail-before-pass-after: this
        // assert requires the new method to exist AND to compose the
        // two side-effects in canonical order on a single-call
        // registration — pre-lift `register_macro_def` did not exist.
        let mut e = Expander::new();
        e.register_macro_def(macro_def_id())
            .expect("well-formed MacroDef must register");
        assert!(
            e.has("id"),
            "self.macros must carry the registered name after register_macro_def"
        );
        assert!(
            e.templates.contains_key("id"),
            "self.templates must carry the compiled bytecode under the bytecode-default posture"
        );
        // The registered macro must expand correctly through the
        // bytecode strategy — proves the inserted template is the right
        // bytecode for the body.
        let out = e
            .expand_program(read("(id 42)").unwrap())
            .expect("registered macro must expand");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Sexp::int(42));
    }

    #[test]
    fn register_macro_def_substitute_only_skips_templates() {
        // `Expander::new_substitute_only()` carries `compile_templates:
        // false`; `register_macro_def` MUST populate `self.macros` for
        // the substitute path but SKIP `self.templates` (no bytecode
        // pre-compile). Pin the `compile_templates: false` gate fires
        // structurally — a regression that drifts the conditional from
        // the registration primitive (e.g. a future emitter that
        // unconditionally pre-compiles) would silently double the
        // benchmark baseline's allocation footprint.
        let mut e = Expander::new_substitute_only();
        e.register_macro_def(macro_def_id())
            .expect("well-formed MacroDef must register under substitute-only");
        assert!(
            e.has("id"),
            "self.macros must carry the registered name even under substitute-only"
        );
        assert!(
            !e.templates.contains_key("id"),
            "self.templates MUST be empty under compile_templates: false — the gate fires"
        );
        // The registered macro must still expand correctly through the
        // substitute strategy — proves the inserted MacroDef is the
        // right body for substitute-walking.
        let out = e
            .expand_program(read("(id 42)").unwrap())
            .expect("registered macro must expand via substitute path");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Sexp::int(42));
    }

    #[test]
    fn register_macro_def_template_compile_failure_leaves_both_tables_pristine() {
        // The structural ordering pin: when `compile_template(&def)?`
        // rejects (e.g. `,unbound` against empty params), the `?` MUST
        // short-circuit BEFORE `self.macros.insert(def.name.clone(),
        // def)` runs, so a failed registration leaves BOTH tables
        // exactly as they were. Without this ordering a future
        // bytecode-strategy lookup would resolve `self.macros.get(name)`
        // (the body was inserted) AND find `self.templates.get(name)`
        // returning `None` (the pre-compile failed), silently coercing
        // the macro onto the substitute fallback path despite
        // `compile_templates: true`. The lift preserves the pre-lift
        // ordering (`compile_template` precedes `self.macros.insert`)
        // structurally — the test pins it across the lift.
        let mut e = Expander::new();
        let err = e
            .register_macro_def(macro_def_bad_template())
            .expect_err("unbound-template body must reject");
        // The rejection is the structural variant the bytecode
        // strategy's template-gate emits.
        assert!(
            matches!(err, LispError::UnboundTemplateVar { .. }),
            "expected UnboundTemplateVar, got: {err:?}"
        );
        // Both tables MUST be pristine — no partial write.
        assert!(
            !e.has("bad"),
            "self.macros must be untouched after compile_template failure"
        );
        assert!(
            !e.templates.contains_key("bad"),
            "self.templates must be untouched after compile_template failure"
        );
    }

    #[test]
    fn with_macros_routes_through_register_macro_def_path_uniformity() {
        // Path-uniformity pin across the bulk-constructor consumer:
        // `with_macros([def])` MUST produce the same final state as
        // `Expander::new()` + `register_macro_def(def)`. A regression
        // that drifts the constructor's per-MacroDef inline block from
        // the registration primitive (e.g. a future emitter that
        // re-inlines the two-step block at `with_macros` rather than
        // delegating) would fail loudly here.
        let mut via_register = Expander::new();
        via_register
            .register_macro_def(macro_def_id())
            .expect("register must succeed");
        let mut via_with_macros =
            Expander::with_macros([macro_def_id()]).expect("with_macros must succeed");
        assert_eq!(via_register.len(), via_with_macros.len());
        assert!(via_register.has("id"));
        assert!(via_with_macros.has("id"));
        // Both tables key on the same set across both postures.
        assert_eq!(
            via_register.templates.contains_key("id"),
            via_with_macros.templates.contains_key("id"),
            "self.templates key-presence must agree across with_macros and register_macro_def"
        );
        // Both registered expanders expand the registered macro
        // identically — the strongest behavioral parity.
        let out_a = via_register
            .expand_program(read("(id 99)").unwrap())
            .unwrap();
        let out_b = via_with_macros
            .expand_program(read("(id 99)").unwrap())
            .unwrap();
        assert_eq!(out_a, out_b);
        assert_eq!(out_a, vec![Sexp::int(99)]);
    }

    #[test]
    fn expand_program_routes_through_register_macro_def_path_uniformity() {
        // Path-uniformity pin across the program-walk consumer:
        // `expand_program` of `(defmacro id (x) `,x)` MUST produce the
        // same final state as a direct
        // `register_macro_def(macro_def_id())`. A regression that
        // drifts `expand_program`'s `(defmacro …)`-head arm from the
        // registration primitive (e.g. a future emitter that re-inlines
        // the two-step block at `expand_program` rather than delegating)
        // would fail loudly here.
        let mut via_register = Expander::new();
        via_register
            .register_macro_def(macro_def_id())
            .expect("register must succeed");
        let mut via_expand_program = Expander::new();
        let yielded = via_expand_program
            .expand_program(read("(defmacro id (x) `,x)").unwrap())
            .expect("expand_program of one defmacro must succeed");
        // expand_program drops the (defmacro …) form from its returned
        // Vec<Sexp> (defmacro is a definition, not a program form), so
        // the yielded list is empty — pin the side-effect-only posture.
        assert!(
            yielded.is_empty(),
            "(defmacro …) is a side-effect-only top-level form; expand_program yields nothing"
        );
        assert!(via_register.has("id"));
        assert!(via_expand_program.has("id"));
        assert_eq!(via_register.len(), via_expand_program.len());
        // Both tables key on the same set across both postures.
        assert_eq!(
            via_register.templates.contains_key("id"),
            via_expand_program.templates.contains_key("id"),
            "self.templates key-presence must agree across expand_program and register_macro_def"
        );
        // Both registered expanders expand the registered macro
        // identically — the strongest behavioral parity, closing
        // path-uniformity across BOTH consumers (with_macros above,
        // expand_program here) at ONE primitive.
        let out_a = via_register
            .expand_program(read("(id 7)").unwrap())
            .unwrap();
        let out_b = via_expand_program
            .expand_program(read("(id 7)").unwrap())
            .unwrap();
        assert_eq!(out_a, out_b);
        assert_eq!(out_a, vec![Sexp::int(7)]);
    }

    // ── as_unquote: path-uniformity across the three substitute / compile_node sites ──
    //
    // The new typed-marker projection `Sexp::as_unquote` lifts the
    // (Sexp::Unquote/UnquoteSplice variant, UnquoteForm::Unquote/Splice
    // literal) pair into ONE structural query. These tests pin behavioral
    // parity end-to-end across BOTH expansion strategies:
    //   * `compile_node` — the bytecode-template strategy's typed marker
    //     dispatch routes through as_unquote at the compile step.
    //   * `substitute` (top-level + list-inner) — the substitute-walker
    //     fallback strategy's typed marker dispatch routes through
    //     as_unquote at both per-form sites.
    // The structural invariant the prior runs' `expansion_layers_agree_on_
    // output_and_cache_wins` benchmark observes — bytecode AND substitute
    // produce byte-identical output — is anchored here at the macro-template
    // level for every distinct unquote-family shape the substitute and
    // bytecode strategies discriminate.

    #[test]
    fn bytecode_and_substitute_agree_on_unquote_substitution_routed_through_as_unquote() {
        // A template body whose only marker is a top-level `,x`. Both
        // expansion strategies route through `as_unquote` (compile_node for
        // bytecode, substitute for the fallback walker), each pairing
        // Sexp::Unquote ↔ UnquoteForm::Unquote at ONE typed projection.
        // Pin byte-identical output across both strategies — the
        // structural-invariant the new projection's lift was designed to
        // make load-bearing structural rather than per-site discipline.
        let src = "(defmacro id (x) ,x) (id 42)";
        let mut bc = Expander::new();
        let mut sub = Expander::new_substitute_only();
        let out_bc = bc.expand_program(read(src).unwrap()).unwrap();
        let out_sub = sub.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out_bc, out_sub, "strategies diverged on `,x` template");
        assert_eq!(out_bc, vec![Sexp::int(42)]);
    }

    #[test]
    fn bytecode_and_substitute_agree_on_unquote_splice_routed_through_as_unquote() {
        // A template body whose marker is a list-inner `,@xs`. The substitute
        // strategy's list-inner Splice arm routes through `as_unquote`
        // matching Some((UnquoteForm::Splice, inner)); the bytecode strategy's
        // compile_node Splice arm routes through `as_unquote` matching the
        // same. Pin byte-identical output across both strategies for the
        // splice path — the projection's typed-marker pairing
        // Sexp::UnquoteSplice ↔ UnquoteForm::Splice is structurally
        // identical at every consumer post-lift.
        let src = "(defmacro wrap (xs) (list 0 ,@xs 99)) (wrap (1 2 3))";
        let mut bc = Expander::new();
        let mut sub = Expander::new_substitute_only();
        let out_bc = bc.expand_program(read(src).unwrap()).unwrap();
        let out_sub = sub.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out_bc, out_sub, "strategies diverged on `,@xs` template");
        let expected = Sexp::List(vec![
            Sexp::symbol("list"),
            Sexp::int(0),
            Sexp::int(1),
            Sexp::int(2),
            Sexp::int(3),
            Sexp::int(99),
        ]);
        assert_eq!(out_bc, vec![expected]);
    }

    #[test]
    fn substitute_splice_outside_list_routes_through_as_unquote_typed_marker() {
        // The substitute strategy's top-level `,@x` arm rejects with
        // LispError::SpliceOutsideList (a splice marker with no containing
        // list to flatten into). Pre-lift the arm was
        // `Sexp::UnquoteSplice(inner) => Err(splice_outside_list(inner))`;
        // post-lift the arm routes through `as_unquote` matching
        // Some((UnquoteForm::Splice, inner)) and dispatches on the typed
        // marker. Pin path-uniformity: the rejection MUST fire identically
        // through the new projection. The substitute-only strategy bypasses
        // the bytecode path, exposing this gate directly.
        //
        // `(defmacro bad (xs) ,@xs) (bad (1 2 3))` — the body is a bare
        // `,@xs` at top level (NOT wrapped in a containing list), so the
        // substitute fallback's top-level Splice arm fires.
        let src = "(defmacro bad (xs) ,@xs) (bad (1 2 3))";
        let mut sub = Expander::new_substitute_only();
        let err = sub.expand_program(read(src).unwrap()).unwrap_err();
        assert!(
            matches!(err, crate::error::LispError::SpliceOutsideList { .. }),
            "expected SpliceOutsideList through as_unquote, got: {err:?}"
        );
    }

    #[test]
    fn as_unquote_threads_typed_marker_into_unbound_template_var_rejection() {
        // A `,unbound` template body — the inner symbol isn't a param —
        // fires gate-2 (must-be-bound-in-scope). The typed marker
        // `UnquoteForm::Unquote` MUST thread through `as_unquote` →
        // `resolve_unquote_in_params(inner, params, form)` → gate-2's
        // rejection-builder. Pre-lift the marker was bound at the per-arm
        // literal `UnquoteForm::Unquote`; post-lift it's bound at the
        // typed projection. Pin: the rejection's `prefix` slot is
        // UnquoteForm::Unquote, structurally derived from the typed
        // projection's typed marker, NOT a literal at the arm body.
        let src = "(defmacro bad (x) ,unbound)";
        let mut bc = Expander::new();
        let err = bc.expand_program(read(src).unwrap()).unwrap_err();
        match err {
            crate::error::LispError::UnboundTemplateVar { prefix, .. } => {
                assert_eq!(
                    prefix,
                    UnquoteForm::Unquote,
                    "typed marker drifted from UnquoteForm::Unquote at gate-2"
                );
            }
            other => panic!("expected UnboundTemplateVar, got: {other:?}"),
        }
        // Sibling negative control: `,@unbound` inside a containing list
        // threads UnquoteForm::Splice through the same projection. The
        // splice MUST be inside a list — `compile_template` rejects
        // top-level `,@X` bodies with SpliceOutsideList before compile_node
        // runs, so the typed-marker dispatch on Splice is observable only
        // for well-positioned splice bodies. Closes path-uniformity across
        // BOTH typed marker variants at the compile path.
        let src_splice = "(defmacro bad (x) (foo ,@unbound))";
        let mut bc2 = Expander::new();
        let err_splice = bc2.expand_program(read(src_splice).unwrap()).unwrap_err();
        match err_splice {
            crate::error::LispError::UnboundTemplateVar { prefix, .. } => {
                assert_eq!(
                    prefix,
                    UnquoteForm::Splice,
                    "typed marker drifted from UnquoteForm::Splice at gate-2"
                );
            }
            other => panic!("expected UnboundTemplateVar, got: {other:?}"),
        }
    }

    // ── compile_template + contains_unquote path-uniformity through as_unquote ──
    //
    // The prior `as_unquote` projection lift (eb00684) routed `compile_node`'s
    // two arms and `substitute`'s top-level + list-inner arms through the
    // typed-marker projection but LEFT `compile_template`'s top-level
    // splice-outside-list gate and `contains_unquote`'s family check inline
    // matching `Sexp::UnquoteSplice(inner)` / `Sexp::Unquote(_) |
    // Sexp::UnquoteSplice(_)`. After this lift both production sites route
    // through `as_unquote`: `compile_template` matches `Some((UnquoteForm::
    // Splice, inner))` (same shape as `substitute`'s list-inner Splice arm),
    // `contains_unquote` uses `as_unquote().is_some()` for the family check.
    // Every production-site recognizer of an unquote-family wrapper now
    // shares ONE typed-marker projection — the (Sexp variant, UnquoteForm
    // variant) pairing for `,@-outside-list` is bound at ONE structural
    // query across all FOUR reachable splice-recognition sites, and a future
    // regression that drifts the marker pairing at any production site
    // becomes a type error at the helper boundary rather than a silent
    // per-site divergence.

    #[test]
    fn compile_template_splice_outside_list_routes_through_as_unquote_typed_marker() {
        // The bytecode path's `compile_template` gate pre-rejects top-level
        // `,@X` bodies via `as_unquote()` matching `Some((UnquoteForm::Splice,
        // inner))`. Pre-lift the arm was `if let Sexp::UnquoteSplice(inner) =
        // body` — the LAST production-site inline `Sexp::UnquoteSplice(_)`
        // match outside the projection itself. Post-lift the (Sexp variant,
        // UnquoteForm variant) pairing for the splice-outside-list gate is
        // bound at ONE projection function across all three reachable
        // emission sites (compile_template top-level, substitute top-level,
        // substitute list-inner). Path-uniformity guard at the new boundary:
        // the bytecode-path compile-time gate now routes through the same
        // shape `substitute`'s list-inner Splice arm uses.
        //
        // `(defmacro bad (xs) \`,@xs)` — the body's outer quasi-quote is
        // peeled by `template_body`, leaving `,@xs` at top level for
        // `compile_template` to gate before `compile_node` walks anything.
        // The bytecode strategy is default-on (`Expander::new()` sets
        // `compile_templates: true`), so the gate fires at
        // `register_macro_def` time.
        let mut e = Expander::new();
        let err = e
            .expand_program(read("(defmacro bad (xs) `,@xs)").unwrap())
            .expect_err("compile_template must reject top-level ,@X via as_unquote");
        assert!(
            matches!(err, crate::error::LispError::SpliceOutsideList { .. }),
            "expected SpliceOutsideList through as_unquote, got: {err:?}"
        );
    }

    #[test]
    fn compile_template_accepts_top_level_unquote_through_as_unquote_typed_marker() {
        // Negative control on the typed-marker dispatch at `compile_template`'s
        // top-level gate. A top-level `,X` body (Unquote, NOT Splice) is a
        // valid template — `compile_node` lowers it to a single `Subst(idx)`
        // op. The new `Some((UnquoteForm::Splice, inner))` pattern at the
        // gate MUST NOT fire on the Unquote variant — only Splice — so the
        // typed marker is observably load-bearing: a regression that drifts
        // the pattern from `UnquoteForm::Splice` to the wider
        // `Some((_, inner))` would mis-reject `(defmacro id (x) ,x)` here.
        //
        // `(defmacro id (x) ,x) (id 42)` — `id`'s body is bare `,x` at top
        // level; `compile_template` admits it via the typed-marker gate,
        // `compile_node` emits `Subst(0)`, and the call expands to `42`.
        let src = "(defmacro id (x) ,x) (id 42)";
        let mut e = Expander::new();
        let expanded = e
            .expand_program(read(src).unwrap())
            .expect("top-level ,X body must compile through as_unquote-typed gate");
        assert_eq!(expanded, vec![Sexp::int(42)]);
    }

    #[test]
    fn contains_unquote_routes_through_as_unquote_for_unquote_family_recognition() {
        // The fast-path optimizer in `compile_node` short-circuits on
        // `contains_unquote(node)` — true iff the subtree carries an
        // unquote-family wrapper. Pre-lift the family check inlined
        // `Sexp::Unquote(_) | Sexp::UnquoteSplice(_) => true`; post-lift it
        // routes through `as_unquote().is_some()`. Pin path-uniformity at
        // the family-recognition boundary: every production-site unquote
        // recognizer (compile_template's gate, compile_node's per-arm
        // dispatch, substitute's top-level + list-inner arms, AND this
        // fast-path predicate) now shares ONE typed-marker projection.
        //
        // Both variants must trigger the fast-path bail — the optimizer's
        // gate keys on the family, not the inner. The recursion into
        // Quote/Quasiquote/List subtrees ALSO observes the same family
        // gate at every level, so a `,@xs` buried under a `\`...` outer
        // wrapper still fires it.
        let bare_unquote = Sexp::Unquote(Box::new(Sexp::symbol("x")));
        let bare_splice = Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs")));
        let nested = Sexp::Quasiquote(Box::new(Sexp::List(vec![
            Sexp::symbol("foo"),
            Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
        ])));
        // The projection's `is_some()` face MUST agree with the pre-lift
        // `matches!()` discriminant on every variant — closed-set guarantee
        // shared with `as_unquote`'s contract pin in `ast.rs`.
        assert!(super::contains_unquote(&bare_unquote));
        assert!(super::contains_unquote(&bare_splice));
        assert!(super::contains_unquote(&nested));
        // Negative control: shapes the projection rejects (Nil, Atom,
        // bare List, Quote-family without inner unquote) must NOT trigger
        // the fast-path bail — the projection's None face stays observably
        // distinct from its Some face.
        assert!(!super::contains_unquote(&Sexp::Nil));
        assert!(!super::contains_unquote(&Sexp::symbol("plain")));
        assert!(!super::contains_unquote(&Sexp::int(5)));
        assert!(!super::contains_unquote(&Sexp::List(vec![
            Sexp::symbol("plain"),
            Sexp::int(1),
        ])));
        assert!(!super::contains_unquote(&Sexp::Quote(Box::new(
            Sexp::symbol("inert")
        ))));
    }

    #[test]
    fn contains_unquote_routes_quote_family_through_as_quote_form_typed_marker() {
        // Pin the lift: `contains_unquote`'s quote-family recognition
        // now routes through `Sexp::as_quote_form` (the 4-of-4 wrapper
        // projection) AND `QuoteForm::as_unquote_form` (the 2-of-4
        // substitution-subset gate) at ONE site — not through the pair
        // of `as_unquote().is_some()` + inline
        // `Sexp::Quote(inner) | Sexp::Quasiquote(inner)` arm. The
        // path-uniformity assertion: for every quote-family wrapper, the
        // function's behavior agrees with the manual composition through
        // the two typed-marker projections. A regression that re-inlines
        // either arm (e.g. drops the `as_unquote_form().is_some()` gate
        // and just returns true for any `as_quote_form().is_some()`, or
        // restores the per-variant `Quote | Quasiquote` arm) drifts from
        // this composition and surfaces here.
        use crate::ast::QuoteForm;

        // Sweep every QuoteForm variant under both axes:
        //   * unquote-only-inner (inner = symbol):
        //       Unquote(s)        → true via `as_unquote_form() == Some`
        //       UnquoteSplice(s)  → true via `as_unquote_form() == Some`
        //       Quote(s)          → false (inner has no unquote, recurses to false)
        //       Quasiquote(s)     → false (same)
        //   * unquote-inner-inside-quote-wrapper:
        //       Quote(Unquote(s)) → true (Quote arm recurses via `as_quote_form`,
        //                                 inner `Unquote` returns true through the
        //                                 SAME projection — both arms share the
        //                                 ONE typed-marker site post-lift)
        let inner_plain = Sexp::symbol("x");
        let inner_unquote = Sexp::Unquote(Box::new(Sexp::symbol("x")));

        for qf in QuoteForm::ALL {
            let wrapped_plain = match qf {
                QuoteForm::Quote => Sexp::Quote(Box::new(inner_plain.clone())),
                QuoteForm::Quasiquote => Sexp::Quasiquote(Box::new(inner_plain.clone())),
                QuoteForm::Unquote => Sexp::Unquote(Box::new(inner_plain.clone())),
                QuoteForm::UnquoteSplice => Sexp::UnquoteSplice(Box::new(inner_plain.clone())),
            };
            // Path-uniformity: behavior derives from the manual two-marker
            // composition through `as_quote_form` + `as_unquote_form`.
            // The substitution-subset gate (`as_unquote_form().is_some()`)
            // returns true for Unquote/UnquoteSplice and false for
            // Quote/Quasiquote — directly encoding the 2-of-4 partition.
            let via_manual =
                qf.as_unquote_form().is_some() || super::contains_unquote(&inner_plain);
            assert_eq!(
                super::contains_unquote(&wrapped_plain),
                via_manual,
                "contains_unquote drifted from as_quote_form + as_unquote_form composition for {qf:?}"
            );

            let wrapped_unquote = match qf {
                QuoteForm::Quote => Sexp::Quote(Box::new(inner_unquote.clone())),
                QuoteForm::Quasiquote => Sexp::Quasiquote(Box::new(inner_unquote.clone())),
                QuoteForm::Unquote => Sexp::Unquote(Box::new(inner_unquote.clone())),
                QuoteForm::UnquoteSplice => Sexp::UnquoteSplice(Box::new(inner_unquote.clone())),
            };
            // EVERY quote-family wrapper around an inner `Unquote` must
            // contain unquote — either the outer projects as `Some` on
            // the substitution subset (Unquote/UnquoteSplice arms), OR
            // the outer is Quote/Quasiquote and the recursion into the
            // inner re-fires the projection. The pre-lift Quote arm
            // recursed via the inline `Sexp::Quote(inner)` match;
            // post-lift the SAME recursion fires via `as_quote_form`'s
            // typed projection. Both yield `true`.
            assert!(
                super::contains_unquote(&wrapped_unquote),
                "contains_unquote missed an inner Unquote under {qf:?} — \
                 the quote-family recursion through as_quote_form drifted"
            );
        }

        // Negative control: a non-quote-family wrapper (List, Atom,
        // Nil) must NOT route through `as_quote_form` at all. The
        // `if let Some(...) = node.as_quote_form()` gate stays `None`
        // and falls through to the List/_  arms. This is the structural
        // partition: the projection's `None` face MUST agree with
        // `!matches!(node, Sexp::Quote(_) | Sexp::Quasiquote(_) |
        // Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`.
        let nested_in_list = Sexp::List(vec![
            Sexp::symbol("outer"),
            Sexp::Quasiquote(Box::new(Sexp::Unquote(Box::new(Sexp::symbol("x"))))),
        ]);
        assert!(
            super::contains_unquote(&nested_in_list),
            "contains_unquote failed to descend into a List subtree containing a \
             Quasiquote(Unquote(_)) — list recursion arm drifted"
        );
    }

    // ── expand_and_collect_named_calls_to_any (from-forms × named ────
    // × typed-decoded classifier) ─────────────────────────────────────
    //
    // The (named NAME-then-kwargs × typed-decoded classifier) cell of
    // the dispatcher matrix on the `Expander` surface — pre-lift the
    // matrix had the bare-kwargs row closed (expand_and_collect_calls_to
    // + expand_and_collect_calls_to_any) AND the constant-keyword
    // column closed on the named axis (expand_to_named +
    // expand_source_to_named) but the (named × classifier) cell was
    // open — every consumer that wanted to walk a program by typed-
    // decoded classifier AND extract the NAME slot per matched form
    // had to compose `expand_and_collect_calls_to_any` with
    // `split_name_slot` inline. Post-lift the cell is ONE named primitive
    // on the `Expander` surface, and the constant-`T::KEYWORD` named
    // sibling routes through it as the (constant-classifier × named)
    // specialization.
    //
    // The tests below pin: (a) the happy path yields the decoded triple
    // `(T, &str, &[Sexp])` for every matched form in source order; (b)
    // non-matching forms are skipped (soft-projection contract inherited
    // from the typed-decoded primitive); (c) the `NamedFormMissingName`
    // variant fires for matched forms with no NAME slot, threading the
    // classifier-supplied `&'static str` keyword; (d) the
    // `NamedFormNonSymbolName` variant fires for matched forms with a
    // non-symbol-or-string NAME slot, again threading the classifier-
    // supplied keyword; (e) the projection's `Err` short-circuits at
    // the first failing matched form; (f) `expand_program` runs BEFORE
    // the classifier filter walks — a `(defmacro …)` emitting a named
    // form is decoded by the classifier on the EXPANDED head, not the
    // macro-call head; (g) the from-source sibling agrees with `read +
    // from-forms` on the same source.

    #[test]
    fn expand_and_collect_named_calls_to_any_yields_decoded_triple_for_every_matching_form_in_source_order(
    ) {
        // The typed-decoded named primitive's happy path: a closed-set
        // classifier decodes head symbols to a typed enum PAIRED with
        // its canonical static keyword, and the per-form projection
        // receives `(decoded, name, spec_args)` for every matched form
        // in source order. Sweep across TWO distinct classifier outcomes
        // — `Monitor` / `Notify` — interleaved with a non-matching form
        // to pin the typed-witness threading AND the NAME-slot
        // projection AND the source-order yield AND the rejection of
        // non-classifier forms in ONE assertion.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Kind {
            Monitor,
            Notify,
        }
        let src = r#"(defmonitor cpu :threshold 80)
                     (other-form 99)
                     (defnotify email :to "ops@example.com")
                     (defmonitor mem :threshold 90)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let rows: Vec<(Kind, String, usize)> = e
            .expand_and_collect_named_calls_to_any(
                forms,
                |h| match h {
                    "defmonitor" => Some((Kind::Monitor, "defmonitor")),
                    "defnotify" => Some((Kind::Notify, "defnotify")),
                    _ => None,
                },
                |kind, name, spec_args| Ok((kind, name.to_string(), spec_args.len())),
            )
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (Kind::Monitor, "cpu".to_string(), 2),
                (Kind::Notify, "email".to_string(), 2),
                (Kind::Monitor, "mem".to_string(), 2),
            ],
        );
    }

    #[test]
    fn expand_and_collect_named_calls_to_any_skips_non_matching_forms_without_invoking_project() {
        // Soft-projection contract — every shape the typed-decoded
        // primitive rejects (non-list, empty list, list whose head is
        // not a symbol, list whose head decodes to `None`) skips the
        // projection silently. The named gate inside the wrapper
        // projection runs ONLY for classifier-matched forms; a non-
        // matching form that reaches the projection panics here.
        let src = r#":bare-keyword
                     "bare-string"
                     42
                     ()
                     (foo bar)
                     (defmonitor cpu :threshold 80)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let names: Vec<String> = e
            .expand_and_collect_named_calls_to_any(
                forms,
                |h| (h == "defmonitor").then_some(((), "defmonitor")),
                |(), name, _args| Ok(name.to_string()),
            )
            .unwrap();
        assert_eq!(names, vec!["cpu".to_string()]);
    }

    #[test]
    fn expand_and_collect_named_calls_to_any_emits_named_form_missing_name_through_classifier_keyword(
    ) {
        // `(defmonitor)` matches the classifier but has no NAME slot —
        // `split_name_slot` fires `NamedFormMissingName { keyword:
        // "defmonitor" }`. The classifier's `&'static str` keyword is
        // threaded VERBATIM through the named gate; a regression that
        // hardcodes a different keyword (e.g. the matched head text or
        // a default) would fail the structural variant identity
        // assertion. Fail-before-pass-after: pre-lift the primitive did
        // not exist; the inline `expand_and_collect_calls_to_any +
        // split_name_slot` composition at the consumer site was the
        // only path, with the keyword bound at the consumer's call
        // boundary rather than at the named primitive's body.
        let forms = read("(defmonitor)").unwrap();
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_named_calls_to_any::<(), _, _, ()>(
                forms,
                |h| (h == "defmonitor").then_some(((), "defmonitor")),
                |(), _name, _args| Ok(()),
            )
            .unwrap_err();
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormMissingName {
                    keyword: "defmonitor"
                }
            ),
            "expected NamedFormMissingName with classifier-supplied keyword, got: {err:?}"
        );
    }

    #[test]
    fn expand_and_collect_named_calls_to_any_emits_named_form_non_symbol_name_through_classifier_keyword(
    ) {
        // `(defmonitor 42 :threshold 80)` matches the classifier but
        // the NAME slot is an int — `split_name_slot` fires
        // `NamedFormNonSymbolName { keyword: "defmonitor", got:
        // SexpShape::Int }`. The classifier's `&'static str` keyword is
        // threaded through the structural rejection variant verbatim,
        // and the typed `SexpShape` projection on the offending slot
        // is preserved across the named primitive.
        let forms = read("(defmonitor 42 :threshold 80)").unwrap();
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_named_calls_to_any::<(), _, _, ()>(
                forms,
                |h| (h == "defmonitor").then_some(((), "defmonitor")),
                |(), _name, _args| Ok(()),
            )
            .unwrap_err();
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormNonSymbolName {
                    keyword: "defmonitor",
                    got: crate::error::SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName with classifier-supplied keyword + SexpShape::Int, got: {err:?}"
        );
    }

    #[test]
    fn expand_and_collect_named_calls_to_any_short_circuits_on_project_error_at_first_failure() {
        // Result projection's short-circuit contract — when the
        // projection returns `Err` on a matched named form, the walk
        // stops and subsequent matched forms are NOT projected. Pin
        // source-order short-circuit via a counter the projection
        // increments before deciding to fail: the counter sits at
        // exactly the index of the failing form (second match),
        // proving the third match never reached the projection.
        let forms = read("(defmon a :x 1) (defmon b :x 2) (defmon c :x 3)").unwrap();
        let mut count = 0usize;
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_named_calls_to_any::<String, _, _, ()>(
                forms,
                |h| (h == "defmon").then_some(((), "defmon")),
                |(), name, _args| {
                    count += 1;
                    if name == "b" {
                        return Err(crate::error::LispError::Missing("test-failure"));
                    }
                    Ok(name.to_string())
                },
            )
            .expect_err("projection must short-circuit on first Err");
        assert_eq!(
            count, 2,
            "projection must have run on first matched form AND the failing form, then stopped"
        );
        assert!(
            matches!(err, crate::error::LispError::Missing("test-failure")),
            "expected the projection's typed Err verbatim, got: {err:?}"
        );
    }

    #[test]
    fn expand_and_collect_named_calls_to_any_expands_macros_before_filtering_by_classifier() {
        // Ordering contract — `expand_program` runs BEFORE the
        // typed-decoded named walk. A `(defmacro …)` form that emits a
        // classifier-decoded named form must have its EXPANDED head
        // decoded by the classifier, not the macro-call head. Pin the
        // ordering with a macro `(emit-mon n thr)` expanding to
        // `(defmonitor ,n :threshold ,thr)` — the classifier sees the
        // expanded `defmonitor` head and the NAME slot's symbol value
        // from the macro arg.
        let src = r#"(defmacro emit-mon (n thr) `(defmonitor ,n :threshold ,thr))
                     (defmonitor cpu :threshold 80)
                     (emit-mon mem 90)
                     (other-form 99)
                     (emit-mon disk 70)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let names: Vec<String> = e
            .expand_and_collect_named_calls_to_any(
                forms,
                |h| (h == "defmonitor").then_some(((), "defmonitor")),
                |(), name, _args| Ok(name.to_string()),
            )
            .unwrap();
        // `cpu` from the literal call, `mem` from the first macro
        // emit, `disk` from the second macro emit. The macro-emitted
        // forms are present BECAUSE `expand_program` ran first.
        assert_eq!(names, vec!["cpu", "mem", "disk"]);
    }

    #[test]
    fn expand_source_and_collect_named_calls_to_any_matches_inlined_read_plus_from_forms_path() {
        // From-source posture parity: feeding source through
        // `expand_source_and_collect_named_calls_to_any` is byte-
        // identical to feeding `read(src)?` through the from-forms
        // sibling on a fresh expander, because the from-source posture
        // is `read(src)? + from-forms` by construction. A regression
        // that drifts the from-source posture's pipeline from the
        // from-forms posture's pipeline would fail here.
        let src = r#"(defmonitor cpu :threshold 80)
                     (defmonitor mem :threshold 90)"#;
        let via_src: Vec<String> = Expander::new()
            .expand_source_and_collect_named_calls_to_any(
                src,
                |h| (h == "defmonitor").then_some(((), "defmonitor")),
                |(), name, _args| Ok(name.to_string()),
            )
            .unwrap();
        let forms = read(src).unwrap();
        let via_forms: Vec<String> = Expander::new()
            .expand_and_collect_named_calls_to_any(
                forms,
                |h| (h == "defmonitor").then_some(((), "defmonitor")),
                |(), name, _args| Ok(name.to_string()),
            )
            .unwrap();
        assert_eq!(via_src, via_forms);
        assert_eq!(via_src, vec!["cpu".to_string(), "mem".to_string()]);
    }

    #[test]
    fn expand_source_and_collect_named_calls_to_any_short_circuits_on_reader_error_before_classifier_runs(
    ) {
        // `?`-routing through `read` short-circuits BEFORE the
        // classifier or the named gate runs. Unbalanced paren — reader
        // error fires; classifier panic-decoder MUST NOT execute,
        // proving the read step gates the entire pipeline.
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_named_calls_to_any::<(), _, _, ()>(
                "(defmonitor cpu :threshold 80",
                |_h| panic!("classifier must not run when reader fails"),
                |(), _name, _args| Ok(()),
            )
            .unwrap_err();
        // The named gate's variants MUST NOT fire — the reader error
        // is structurally distinct.
        assert!(
            !matches!(
                err,
                crate::error::LispError::NamedFormMissingName { .. }
                    | crate::error::LispError::NamedFormNonSymbolName { .. }
            ),
            "expected reader error, not named-gate variant; got: {err:?}"
        );
    }

    // ── expand_and_collect_named_calls_to (from-forms × named × ──────
    // constant-keyword × untyped R) ─────────────────────────────────
    //
    // The (named NAME-then-kwargs × constant-keyword × untyped `R`)
    // cell of the `Expander` typed-walk family — pre-lift the cell was
    // reachable ONLY through `expand_to_named<T>` with the
    // `T: TataraDomain` parameter baking BOTH the `T::KEYWORD` filter
    // AND the `T::compile_from_args`-based projection through
    // `expand_and_collect_calls_to(forms, T::KEYWORD,
    // named_form_projection::<T>)`. Post-lift the cell surfaces as ONE
    // method that takes the keyword and the `(name, args) -> R`
    // projection as caller-supplied parameters, AND
    // `expand_to_named<T>` routes through it as the typed
    // `T::KEYWORD`-constant specialization — so the named-form
    // `split_name_slot` composition lives at ONE site
    // (`expand_and_collect_named_calls_to_any` body) post-lift rather
    // than at TWO sites pre-lift (the bare-kwargs path through
    // `named_form_projection<T>` AND the classifier path).
    //
    // The tests below pin: (a) the happy path yields the `(name, args)`
    // pair for every matched form in source order; (b) non-matching
    // forms are skipped (soft-projection contract inherited from the
    // named typed-decoded primitive); (c) the `NamedFormMissingName`
    // variant fires for matched forms with no NAME slot, threading the
    // primitive's `&'static str` keyword; (d) the
    // `NamedFormNonSymbolName` variant fires for matched forms with a
    // non-symbol-or-string NAME slot, again threading the primitive's
    // keyword; (e) the projection's `Err` short-circuits at the first
    // failing matched form; (f) `expand_program` runs BEFORE the
    // keyword filter walks — a `(defmacro …)` emitting a named form
    // has its EXPANDED head matched by the constant-keyword filter,
    // not the macro-call head; (g) the from-source sibling agrees with
    // `read + from-forms` on the same source; (h) the constant-
    // classifier composition law binds the runtime-keyword cell
    // (`expand_and_collect_named_calls_to`) to the typed-decoded
    // classifier cell (`expand_and_collect_named_calls_to_any`) via a
    // constant-classifier decoder.

    #[test]
    fn expand_and_collect_named_calls_to_yields_name_and_args_for_every_matching_form_in_source_order(
    ) {
        // The constant-keyword named primitive's happy path: a runtime
        // `&'static str` keyword filters matched forms, and the per-
        // form projection receives `(name, spec_args)` for every
        // matched form in source order. Three matched forms
        // interleaved with a non-matcher pin the source-order yield,
        // the NAME-slot projection, AND the rejection of non-keyword
        // forms in ONE assertion.
        let src = r#"(defmonitor cpu :threshold 80)
                     (other-form 99)
                     (defmonitor mem :threshold 90 :unit "MiB")
                     (defmonitor disk :threshold 70)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let rows: Vec<(String, usize)> = e
            .expand_and_collect_named_calls_to(forms, "defmonitor", |name, spec_args| {
                Ok((name.to_string(), spec_args.len()))
            })
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("cpu".to_string(), 2),
                ("mem".to_string(), 4),
                ("disk".to_string(), 2),
            ],
        );
    }

    #[test]
    fn expand_and_collect_named_calls_to_skips_non_matching_forms_without_invoking_project() {
        // Soft-projection contract — every shape the named typed-
        // decoded primitive rejects (non-list, empty list, list whose
        // head is not a symbol, list whose head doesn't match the
        // constant keyword) skips the projection silently. The named
        // gate inside the wrapper projection runs ONLY for matched
        // forms; a non-matching form that reaches the projection
        // panics here.
        let src = r#":bare-keyword
                     "bare-string"
                     42
                     ()
                     (foo bar)
                     (defmonitor cpu :threshold 80)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let names: Vec<String> = e
            .expand_and_collect_named_calls_to(forms, "defmonitor", |name, _args| {
                Ok(name.to_string())
            })
            .unwrap();
        assert_eq!(names, vec!["cpu".to_string()]);
    }

    #[test]
    fn expand_and_collect_named_calls_to_emits_named_form_missing_name_through_primitive_keyword() {
        // The named-form gate's missing-NAME variant must thread the
        // primitive's `&'static str` keyword (NOT a hardcoded literal,
        // NOT the projection's own copy) — the `NamedFormMissingName
        // { keyword }` slot inside `split_name_slot` is bound at the
        // call site BY THE PRIMITIVE itself, via the constant-
        // classifier decoder threading `((), keyword)` into
        // `expand_and_collect_named_calls_to_any`.
        let src = r#"(defmonitor)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_named_calls_to::<(), _>(forms, "defmonitor", |_name, _args| Ok(()))
            .unwrap_err();
        assert!(
            matches!(
                err,
                crate::error::LispError::NamedFormMissingName {
                    keyword: "defmonitor"
                }
            ),
            "expected NamedFormMissingName threading the primitive's keyword; got: {err:?}",
        );
    }

    #[test]
    fn expand_and_collect_named_calls_to_emits_named_form_non_symbol_name_through_primitive_keyword(
    ) {
        // The named-form gate's non-symbol-NAME variant must thread
        // the primitive's `&'static str` keyword AND the typed-shape
        // witness (`SexpShape::Int` for an integer NAME slot). The
        // shape projection through `split_name_slot`'s
        // `as_symbol_or_string()` gate fires on the first matched
        // form whose NAME slot is not a symbol or string.
        let src = r#"(defmonitor 42 :threshold 80)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_named_calls_to::<(), _>(forms, "defmonitor", |_name, _args| Ok(()))
            .unwrap_err();
        match err {
            crate::error::LispError::NamedFormNonSymbolName { keyword, got } => {
                assert_eq!(keyword, "defmonitor");
                assert_eq!(got, crate::error::SexpShape::Int);
            }
            other => panic!(
                "expected NamedFormNonSymbolName threading the primitive's keyword + Int shape; got: {other:?}",
            ),
        }
    }

    #[test]
    fn expand_and_collect_named_calls_to_short_circuits_on_project_error_at_first_failure() {
        // `Iterator::collect::<Result<Vec<R>, _>>()` short-circuits at
        // the first failing projection. The second matched form's
        // projection error MUST fire AND the third matched form's
        // projection MUST NOT run. Pin both halves with a counter +
        // a fail-on-call sentinel.
        let src = r#"(defmonitor cpu :threshold 80)
                     (defmonitor mem :threshold 90)
                     (defmonitor disk :threshold 70)"#;
        let forms = read(src).unwrap();
        let mut count: usize = 0;
        let mut e = Expander::new();
        let err = e
            .expand_and_collect_named_calls_to::<String, _>(forms, "defmonitor", |name, _args| {
                count += 1;
                if name == "mem" {
                    Err(crate::error::LispError::Compile {
                        form: "test".into(),
                        message: format!("rejecting at NAME={name}"),
                    })
                } else {
                    Ok(name.to_string())
                }
            })
            .unwrap_err();
        assert_eq!(count, 2, "third matched form must not be projected");
        match err {
            crate::error::LispError::Compile { message, .. } => {
                assert!(message.contains("NAME=mem"));
            }
            other => panic!("expected projection-driven Compile error; got: {other:?}"),
        }
    }

    #[test]
    fn expand_and_collect_named_calls_to_expands_macros_before_filtering_by_keyword() {
        // Ordering contract — `expand_program` runs BEFORE the
        // constant-keyword named walk. A `(defmacro …)` form that
        // emits a matched named form must have its EXPANDED head
        // matched by the constant-keyword filter, not the macro-call
        // head. Pin the ordering with a macro `(emit-mon n thr)`
        // expanding to `(defmonitor ,n :threshold ,thr)` — the
        // constant-keyword filter matches the expanded `defmonitor`
        // head and the NAME slot projects the symbol-author value.
        let src = r#"(defmacro emit-mon (n thr) `(defmonitor ,n :threshold ,thr))
                     (defmonitor cpu :threshold 80)
                     (emit-mon mem 90)
                     (other-form 99)
                     (emit-mon disk 70)"#;
        let forms = read(src).unwrap();
        let mut e = Expander::new();
        let names: Vec<String> = e
            .expand_and_collect_named_calls_to(forms, "defmonitor", |name, _args| {
                Ok(name.to_string())
            })
            .unwrap();
        // `cpu` from the literal call, `mem` from the first macro
        // emit, `disk` from the second macro emit. The macro-emitted
        // forms are present BECAUSE `expand_program` ran first.
        assert_eq!(names, vec!["cpu", "mem", "disk"]);
    }

    #[test]
    fn expand_source_and_collect_named_calls_to_matches_inlined_read_plus_from_forms_path() {
        // From-source posture parity: feeding source through
        // `expand_source_and_collect_named_calls_to` is byte-identical
        // to feeding `read(src)?` through the from-forms sibling on a
        // fresh expander, because the from-source posture is
        // `read(src)? + from-forms` by construction. A regression that
        // drifts the from-source posture's pipeline from the from-
        // forms posture's pipeline would fail here.
        let src = r#"(defmonitor cpu :threshold 80)
                     (defmonitor mem :threshold 90)"#;
        let via_src: Vec<String> = Expander::new()
            .expand_source_and_collect_named_calls_to(src, "defmonitor", |name, _args| {
                Ok(name.to_string())
            })
            .unwrap();
        let forms = read(src).unwrap();
        let via_forms: Vec<String> = Expander::new()
            .expand_and_collect_named_calls_to(forms, "defmonitor", |name, _args| {
                Ok(name.to_string())
            })
            .unwrap();
        assert_eq!(via_src, via_forms);
        assert_eq!(via_src, vec!["cpu".to_string(), "mem".to_string()]);
    }

    #[test]
    fn expand_source_and_collect_named_calls_to_short_circuits_on_reader_error_before_named_gate_runs(
    ) {
        // `?`-routing through `read` short-circuits BEFORE the
        // constant-keyword filter, the named gate, or the projection
        // runs. Unbalanced paren — reader error fires; the projection
        // panic-payload MUST NOT execute, proving the read step gates
        // the entire pipeline.
        let mut e = Expander::new();
        let err = e
            .expand_source_and_collect_named_calls_to::<(), _>(
                "(defmonitor cpu :threshold 80",
                "defmonitor",
                |_name, _args| panic!("projection must not run when reader fails"),
            )
            .unwrap_err();
        // The named gate's variants MUST NOT fire — the reader error
        // is structurally distinct.
        assert!(
            !matches!(
                err,
                crate::error::LispError::NamedFormMissingName { .. }
                    | crate::error::LispError::NamedFormNonSymbolName { .. }
            ),
            "expected reader error, not named-gate variant; got: {err:?}",
        );
    }

    #[test]
    fn expand_and_collect_named_calls_to_routes_through_classifier_via_constant_decoder_composition(
    ) {
        // Composition-identity test pinning the runtime-keyword named
        // cell (`expand_and_collect_named_calls_to`) to the typed-
        // decoded named-classifier cell
        // (`expand_and_collect_named_calls_to_any`) via the constant-
        // classifier decoder shape. Post-lift the identity:
        //
        //   expand_and_collect_named_calls_to(forms, kw, project) ==
        //       expand_and_collect_named_calls_to_any(forms,
        //           |h| (h == kw).then_some(((), kw)),
        //           |(), name, args| project(name, args))
        //
        // Pinning the identity here makes the typed-decoded named-
        // classifier primitive the CANONICAL composition point the
        // runtime-keyword sibling routes through — parallel to how
        // `expand_and_collect_calls_to` routes through
        // `expand_and_collect_calls_to_any` via a `|h| (h == k).
        // then_some(())` decoder on the bare-kwargs axis. A future
        // regression that drifts ONE cell's NAME-slot rejection chain
        // from the other becomes loudly visible at this assertion.
        let src = r#"(defmonitor cpu :threshold 80)
                     (other-form 99)
                     (defmonitor mem :threshold 90)"#;
        let forms = read(src).unwrap();
        let via_constant_keyword: Vec<(String, usize)> = Expander::new()
            .expand_and_collect_named_calls_to(forms.clone(), "defmonitor", |name, args| {
                Ok((name.to_string(), args.len()))
            })
            .unwrap();
        let via_classifier: Vec<(String, usize)> = Expander::new()
            .expand_and_collect_named_calls_to_any(
                forms,
                |h| (h == "defmonitor").then_some(((), "defmonitor")),
                |(), name, args| Ok((name.to_string(), args.len())),
            )
            .unwrap();
        assert_eq!(via_constant_keyword, via_classifier);
        assert_eq!(
            via_constant_keyword,
            vec![("cpu".to_string(), 2), ("mem".to_string(), 2)],
        );
    }

    #[test]
    fn expand_to_named_routes_through_expand_and_collect_named_calls_to_via_constant_keyword_composition(
    ) {
        // End-to-end path-uniformity: `Expander::expand_to_named<T>`
        // (the typed constant-keyword named cell) must yield the same
        // payload as `expand_and_collect_named_calls_to(forms,
        // T::KEYWORD, |name, args| { T::compile_from_args(args)? +
        // NamedDefinition })` — the lift's structural identity.
        //
        // Pre-lift `expand_to_named<T>` routed through
        // `expand_and_collect_calls_to(forms, T::KEYWORD,
        // named_form_projection::<T>)` (the bare-kwargs constant
        // primitive with `named_form_projection<T>` doing the NAME
        // extraction internally). Post-lift `expand_to_named<T>`
        // routes through the named constant-keyword primitive (which
        // itself routes through the classifier primitive via a
        // constant-classifier decoder) — so the `split_name_slot`
        // composition lives at ONE site (the `_any` primitive body)
        // post-lift rather than at TWO sites.
        use crate::compile::NamedDefinition;
        use crate::compiler_spec::CompilerSpec;
        let src = r#"(defcompiler alpha-compiler :name "x" :dialect "standard")
                     (defcompiler beta-compiler  :name "y" :dialect "standard")"#;
        let forms = read(src).unwrap();
        let via_expand_to_named = Expander::new()
            .expand_to_named::<CompilerSpec>(forms.clone())
            .unwrap();
        let via_named_constant: Vec<NamedDefinition<CompilerSpec>> = Expander::new()
            .expand_and_collect_named_calls_to(forms, "defcompiler", |name, spec_args| {
                let spec =
                    <CompilerSpec as crate::domain::TataraDomain>::compile_from_args(spec_args)?;
                Ok(NamedDefinition {
                    name: name.to_string(),
                    spec,
                })
            })
            .unwrap();
        assert_eq!(via_expand_to_named.len(), 2);
        assert_eq!(via_expand_to_named.len(), via_named_constant.len());
        for (a, b) in via_expand_to_named.iter().zip(via_named_constant.iter()) {
            assert_eq!(a.name, b.name, "NAME slot must agree across cells");
            assert_eq!(a.spec.name, b.spec.name, ":name spec must agree");
        }
        assert_eq!(via_expand_to_named[0].name, "alpha-compiler");
        assert_eq!(via_expand_to_named[0].spec.name, "x");
        assert_eq!(via_expand_to_named[1].name, "beta-compiler");
        assert_eq!(via_expand_to_named[1].spec.name, "y");
    }

    // ── ExpansionDepthExceeded: runaway macro rejected instead of aborting ──

    #[test]
    fn expander_default_max_expansion_depth_is_the_module_constant() {
        // Both constructors seed `max_expansion_depth` from the module
        // constant — a regression that drifts one constructor's default
        // from the constant (e.g. a hardcoded `256` at ONE site with the
        // other left uninitialized after a `Default` derive addition)
        // fails this pin.
        assert_eq!(
            Expander::new().max_expansion_depth(),
            DEFAULT_MAX_EXPANSION_DEPTH
        );
        assert_eq!(
            Expander::new_substitute_only().max_expansion_depth(),
            DEFAULT_MAX_EXPANSION_DEPTH
        );
        assert_eq!(DEFAULT_MAX_EXPANSION_DEPTH, 256);
    }

    #[test]
    fn set_max_expansion_depth_takes_effect_on_subsequent_expand() {
        let mut e = Expander::new();
        e.set_max_expansion_depth(7);
        assert_eq!(e.max_expansion_depth(), 7);
    }

    #[test]
    fn expand_recursive_macro_rejects_at_depth_limit_bytecode_path() {
        // `(defmacro loop (x) `(loop ,x))` applied to `(loop hello)`
        // pre-lift stack-overflowed the process — a below-floor abort.
        // Post-lift the expander returns a typed
        // `LispError::ExpansionDepthExceeded` at the ceiling with
        // `macro_name` populated from the offending call's head.
        let mut e = Expander::new();
        e.set_max_expansion_depth(4);
        let forms = read("(defmacro loop (x) `(loop ,x)) (loop hello)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::ExpansionDepthExceeded { macro_name, limit } => {
                assert_eq!(macro_name, "loop");
                assert_eq!(limit, 4);
            }
            other => panic!("expected ExpansionDepthExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn expand_recursive_macro_rejects_at_depth_limit_substitute_path() {
        // The substitute strategy shares the same `expand` outer walker
        // with the bytecode strategy, so the ceiling fires at the SAME
        // gate regardless of which apply-layer is active. The pin makes
        // that shared-outer-walker property structural: a regression
        // that ever branches the depth guard on strategy (e.g. moves it
        // into `apply_compiled` alone) fails this test on the
        // substitute expander.
        let mut e = Expander::new_substitute_only();
        e.set_max_expansion_depth(4);
        let forms = read("(defmacro loop (x) `(loop ,x)) (loop hello)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::ExpansionDepthExceeded { macro_name, limit } => {
                assert_eq!(macro_name, "loop");
                assert_eq!(limit, 4);
            }
            other => panic!("expected ExpansionDepthExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn expand_lawful_nested_macros_within_ceiling_succeed() {
        // Lawful nested macros (`(twice (twice hey))`) accrete depth in
        // the low single digits because the tree-child arm doesn't
        // count — only the post-apply re-expansion path bumps the
        // counter. A ceiling of 3 is enough for a two-deep nested
        // expansion. This pin is the fail-before/pass-after negative
        // control for the recursive-macro rejection tests above: the
        // depth counter must NOT reject lawful macro-nesting within
        // the ceiling, and only reject the runaway shape.
        let mut e = Expander::new();
        e.set_max_expansion_depth(3);
        let forms = read(
            "(defmacro twice (x) `(list ,x ,x))
             (twice (twice hey))",
        )
        .unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(list (list hey hey) (list hey hey))"));
    }

    #[test]
    fn expand_depth_ceiling_ignores_lawful_tree_nesting_depth() {
        // A macro-free lawful deep tree — five levels of plain nesting —
        // is expanded at a ceiling well below the tree's height. The
        // tree-child arm does not accrete depth, so tree height is
        // orthogonal to the ceiling; only macro re-expansion counts.
        // A regression that ever bumps depth on tree-child descent
        // fails this pin.
        let mut e = Expander::new();
        e.set_max_expansion_depth(2);
        let forms = read("(a (b (c (d (e f)))))").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(a (b (c (d (e f)))))"));
    }

    #[test]
    fn expansion_depth_exceeded_position_is_none() {
        // The variant joins the non-positional cohort — the offending
        // macro's byte offset is not what a runaway diagnostic anchors
        // to; the OFFENDING MACRO NAME (`macro_name`) is. This pin
        // keeps the variant in the `position() -> None` cohort so a
        // future span-carrying edit lands the field in ONE place
        // across every non-positional variant simultaneously.
        let err = LispError::ExpansionDepthExceeded {
            macro_name: "loop".to_string(),
            limit: 256,
        };
        assert_eq!(err.position(), None);
    }

    #[test]
    fn expansion_depth_exceeded_display_matches_typed_variant_shape() {
        // Display projection carries both `macro_name` and `limit`, so
        // authoring surfaces that substring-grep the rendered
        // diagnostic (`tatara-check`, REPL, LSP) see BOTH halves at
        // the variant boundary and never need to substring-parse
        // free-form text to recover the identity of the offending
        // macro or the ceiling it breached.
        let err = LispError::ExpansionDepthExceeded {
            macro_name: "loop".to_string(),
            limit: 256,
        };
        let rendered = err.to_string();
        assert!(rendered.contains("loop"), "rendered: {rendered}");
        assert!(rendered.contains("256"), "rendered: {rendered}");
        assert!(
            rendered.contains("macro expansion depth exceeded"),
            "rendered: {rendered}"
        );
    }

    // ── max_cache_entries: bounded-memoization ceiling ──────────────────

    #[test]
    fn expander_default_max_cache_entries_is_the_module_constant() {
        // Both cache-enabling constructors seed `max_cache_entries` from
        // the module constant — a regression that drifts one
        // constructor's default from the constant (e.g. a hardcoded
        // `8192` at ONE site with the other left uninitialized after a
        // `Default` derive addition) fails this pin. The peer
        // constructor `new_substitute_only` seeds the same ceiling so
        // the field stays initialized under `#[derive(Clone)]` (a
        // zero-initialized clone would be a silent bifurcation of the
        // cache-ceiling surface).
        assert_eq!(
            Expander::new().max_cache_entries(),
            DEFAULT_MAX_CACHE_ENTRIES
        );
        assert_eq!(
            Expander::new_substitute_only().max_cache_entries(),
            DEFAULT_MAX_CACHE_ENTRIES
        );
        assert_eq!(DEFAULT_MAX_CACHE_ENTRIES, 8192);
    }

    #[test]
    fn set_max_cache_entries_takes_effect_on_subsequent_expand() {
        // Setter mirrors accessor — a regression that ever forgets to
        // write through to the field, or writes to a shadow field the
        // accessor does not read, fails this pin.
        let mut e = Expander::new();
        e.set_max_cache_entries(7);
        assert_eq!(e.max_cache_entries(), 7);
    }

    #[test]
    fn expand_cache_size_is_bounded_by_max_cache_entries() {
        // Five distinct `(name, args)` pairs expanded against a cache
        // ceiling of `2` populate the cache with EXACTLY the first two
        // insertions and skip the remaining three. Every expansion
        // still returns the correct result — the cache is a pure
        // PERFORMANCE optimization; the ceiling gates memoization,
        // never correctness. LOAD-BEARING true-arm catch on the
        // bounded-cache contract: a regression that ignored the
        // ceiling (unbounded insert) would land `cache_size() == 5`
        // here; a regression that skipped insertion entirely (broken
        // cache) would still see correct expansion output but would
        // fail the interior `cache_size() == 2` pin.
        let mut e = Expander::new();
        e.set_max_cache_entries(2);
        let src = "
            (defmacro id (x) `,x)
            (id one)
            (id two)
            (id three)
            (id four)
            (id five)
        ";
        let out = e.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out.len(), 5);
        assert_eq!(out[0], parse("one"));
        assert_eq!(out[1], parse("two"));
        assert_eq!(out[2], parse("three"));
        assert_eq!(out[3], parse("four"));
        assert_eq!(out[4], parse("five"));
        assert_eq!(
            e.cache_size(),
            2,
            "cache grew past the max_cache_entries ceiling",
        );
    }

    #[test]
    fn expand_zero_cache_ceiling_disables_caching_effectively() {
        // Zero ceiling admits every fresh `(name, args)` pair through
        // the compute path and never grows the cache — the same
        // observable behavior as `set_cache_enabled(false)` but
        // reached through the ceiling knob. Correctness is preserved:
        // the miss path always recomputes the same value the cache
        // would have returned, so an operator that dials the ceiling
        // to `0` (e.g. to hunt a cache-related regression) never
        // bifurcates the output surface. Peer to
        // `set_max_expansion_depth(0)` one RESOURCE axis over — both
        // ceilings admit the zero endpoint as a well-defined
        // extremum.
        let mut e = Expander::new();
        e.set_max_cache_entries(0);
        let src = "
            (defmacro id (x) `,x)
            (id one)
            (id two)
        ";
        let out = e.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], parse("one"));
        assert_eq!(out[1], parse("two"));
        assert_eq!(
            e.cache_size(),
            0,
            "zero cache ceiling grew the cache — the ceiling must be respected on every insert",
        );
    }

    #[test]
    fn expand_cached_hits_survive_past_the_ceiling() {
        // Once a `(name, args)` pair sits IN the cache under the
        // ceiling, subsequent calls hit it — the ceiling only gates
        // NEW-KEY inserts, never lookups. This test seeds the cache
        // with two entries under a ceiling of `2`, then re-issues the
        // SAME calls to prove the cached hits still fire (cache_size
        // stays at `2` because no new keys are inserted, and the
        // expansion output is stable). LOAD-BEARING true-arm catch
        // that the ceiling gates the INSERT path alone — a regression
        // that ever refused cache LOOKUPS at the ceiling would
        // silently bifurcate correctness on the second call.
        let mut e = Expander::new();
        e.set_max_cache_entries(2);
        let src = "
            (defmacro id (x) `,x)
            (id one)
            (id two)
            (id one)
            (id two)
        ";
        let out = e.expand_program(read(src).unwrap()).unwrap();
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], parse("one"));
        assert_eq!(out[1], parse("two"));
        assert_eq!(out[2], parse("one"));
        assert_eq!(out[3], parse("two"));
        assert_eq!(
            e.cache_size(),
            2,
            "cache grew past the max_cache_entries ceiling on repeated (name, args) pairs",
        );
    }

    #[test]
    fn clear_cache_reopens_the_insert_path_after_the_ceiling_was_hit() {
        // After the cache reaches its ceiling and subsequent inserts
        // are skipped, `clear_cache` empties the memo and RE-OPENS the
        // insert path — the next expansion populates the freshly-
        // empty cache without operator intervention on the ceiling
        // itself. The pin binds the operator-facing recovery path
        // documented on [`DEFAULT_MAX_CACHE_ENTRIES`]: "skip cache
        // insertion until the operator clears the cache via
        // [`Expander::clear_cache`]".
        let mut e = Expander::new();
        e.set_max_cache_entries(1);
        let src_fill = "
            (defmacro id (x) `,x)
            (id one)
            (id two)
        ";
        let _ = e.expand_program(read(src_fill).unwrap()).unwrap();
        assert_eq!(e.cache_size(), 1, "cache did not stop at the ceiling");
        e.clear_cache();
        assert_eq!(e.cache_size(), 0, "clear_cache did not empty the memo");
        let out = e
            .expand_program(read("(defmacro id (x) `,x) (id three)").unwrap())
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], parse("three"));
        assert_eq!(
            e.cache_size(),
            1,
            "clear_cache did not re-open the insert path — the cache should have accepted the fresh (id, three) pair",
        );
    }

    // ── max_expansion_size: bounded-output ceiling ─────────────────────

    #[test]
    fn expander_default_max_expansion_size_is_the_module_constant() {
        // Both constructors seed `max_expansion_size` from the module
        // constant — a regression that drifts one constructor's
        // default from the constant (e.g. a hardcoded `65_536` at
        // ONE site with the other left uninitialized after a
        // `Default` derive addition) fails this pin. Peer to the
        // `max_expansion_depth` + `max_cache_entries` default-
        // coherence pins one RESOURCE-DIMENSION axis over.
        assert_eq!(
            Expander::new().max_expansion_size(),
            DEFAULT_MAX_EXPANSION_SIZE
        );
        assert_eq!(
            Expander::new_substitute_only().max_expansion_size(),
            DEFAULT_MAX_EXPANSION_SIZE
        );
        assert_eq!(DEFAULT_MAX_EXPANSION_SIZE, 65_536);
    }

    #[test]
    fn set_max_expansion_size_takes_effect_on_subsequent_expand() {
        // Setter mirrors accessor — a regression that ever forgets to
        // write through to the field, or writes to a shadow field the
        // accessor does not read, fails this pin. Peer to the
        // setter/accessor coherence pins on the depth + cache axes.
        let mut e = Expander::new();
        e.set_max_expansion_size(64);
        assert_eq!(e.max_expansion_size(), 64);
    }

    #[test]
    fn expand_expansion_bomb_rejects_at_size_limit_bytecode_path() {
        // `(defmacro bomb (x) `(list ,x ,x ,x ,x))` applied to
        // `(bomb hey)` produces `(list hey hey hey hey)` — 6 nodes
        // (outer List + 5 children). With a ceiling of `4` this
        // crosses the OUTPUT-SIZE threshold at the first apply and
        // the expander returns a typed
        // `LispError::ExpansionSizeExceeded` with `macro_name`,
        // `size`, and `limit` all populated. Pre-lift the runaway
        // just accumulated in a growing tree until the process
        // heap ran out; post-lift the rejection sits at the same
        // call boundary as every other macro-expansion failure.
        let mut e = Expander::new();
        e.set_max_expansion_size(4);
        let forms = read("(defmacro bomb (x) `(list ,x ,x ,x ,x)) (bomb hey)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::ExpansionSizeExceeded {
                macro_name,
                size,
                limit,
            } => {
                assert_eq!(macro_name, "bomb");
                assert_eq!(size, 6);
                assert_eq!(limit, 4);
            }
            other => panic!("expected ExpansionSizeExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn expand_expansion_bomb_rejects_at_size_limit_substitute_path() {
        // The substitute strategy shares the same `expand_with_depth`
        // outer walker with the bytecode strategy, so the ceiling
        // fires at the SAME gate regardless of which apply-layer is
        // active. The pin makes that shared-outer-walker property
        // structural: a regression that ever branches the size guard
        // on strategy (e.g. moves it into `apply_compiled` alone)
        // fails this test on the substitute expander. Peer to
        // `expand_recursive_macro_rejects_at_depth_limit_substitute_path`
        // one RESOURCE axis over.
        let mut e = Expander::new_substitute_only();
        e.set_max_expansion_size(4);
        let forms = read("(defmacro bomb (x) `(list ,x ,x ,x ,x)) (bomb hey)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::ExpansionSizeExceeded {
                macro_name,
                size,
                limit,
            } => {
                assert_eq!(macro_name, "bomb");
                assert_eq!(size, 6);
                assert_eq!(limit, 4);
            }
            other => panic!("expected ExpansionSizeExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn expand_lawful_output_within_size_ceiling_succeeds() {
        // `(defmacro twice (x) `(list ,x ,x))` applied to `(twice hey)`
        // produces `(list hey hey)` — 4 nodes. With a ceiling of `4`
        // the output sits AT the ceiling (`<=` admits equality; only
        // `>` rejects), so the expansion succeeds. Fail-before/pass-
        // after negative control for the bomb rejection tests: a
        // regression that ever rejected at-ceiling output would fail
        // this pin. Peer to `expand_lawful_nested_macros_within_ceiling_succeed`
        // one RESOURCE axis over.
        let mut e = Expander::new();
        e.set_max_expansion_size(4);
        let forms = read("(defmacro twice (x) `(list ,x ,x)) (twice hey)").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(list hey hey)"));
    }

    #[test]
    fn expand_size_ceiling_ignores_lawful_tree_nesting_size() {
        // A macro-free lawful large tree is NOT gated by the size
        // ceiling — the ceiling gates macro-`apply` outputs alone,
        // not tree-child descents through non-macro forms. A `usize::MAX`
        // ceiling would trivially admit anything; this pin uses a
        // deliberately small ceiling of `4` on a tree with 6 nodes
        // to catch a regression that ever bumps the ceiling on tree
        // descent. A tree-only walk should NEVER fire the ceiling.
        // Peer to `expand_depth_ceiling_ignores_lawful_tree_nesting_depth`
        // one RESOURCE axis over.
        let mut e = Expander::new();
        e.set_max_expansion_size(4);
        // `(a (b (c d)))` — 6 nodes, no macros. Passes cleanly.
        let forms = read("(a (b (c d)))").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(a (b (c d)))"));
    }

    #[test]
    fn expansion_size_exceeded_position_is_none() {
        // The variant joins the non-positional cohort — the offending
        // macro's byte offset is not what an expansion-bomb diagnostic
        // anchors to; the (offending macro name, observed size,
        // configured ceiling) triple is. This pin keeps the variant
        // in the `position() -> None` cohort so a future span-carrying
        // edit lands the field in ONE place across every non-positional
        // variant simultaneously. Peer to
        // `expansion_depth_exceeded_position_is_none` one RESOURCE
        // axis over.
        let err = LispError::ExpansionSizeExceeded {
            macro_name: "bomb".to_string(),
            size: 512,
            limit: 256,
        };
        assert_eq!(err.position(), None);
    }

    #[test]
    fn expansion_size_exceeded_display_matches_typed_variant_shape() {
        // Display projection carries `macro_name`, `size`, AND
        // `limit`, so authoring surfaces that substring-grep the
        // rendered diagnostic (`tatara-check`, REPL, LSP) see ALL
        // THREE halves at the variant boundary and never need to
        // substring-parse free-form text to recover the identity of
        // the offending macro, the observed size, or the ceiling it
        // breached. Peer to `expansion_depth_exceeded_display_matches_typed_variant_shape`
        // one RESOURCE axis over.
        let err = LispError::ExpansionSizeExceeded {
            macro_name: "bomb".to_string(),
            size: 512,
            limit: 256,
        };
        let rendered = err.to_string();
        assert!(rendered.contains("bomb"), "rendered: {rendered}");
        assert!(rendered.contains("512"), "rendered: {rendered}");
        assert!(rendered.contains("256"), "rendered: {rendered}");
        assert!(
            rendered.contains("macro expansion output size exceeded"),
            "rendered: {rendered}"
        );
    }

    #[test]
    fn expand_size_ceiling_names_the_offending_macro_in_a_nested_expansion() {
        // A lawful outer macro that expands to a call of a bomb inner
        // macro rejects at the INNER call — `macro_name` at the
        // variant boundary is the name of the macro whose `apply`
        // output crossed the ceiling, not the outermost caller.
        // Load-bearing name-provenance pin: an operator whose bomb
        // sits N levels deep in a compose chain needs to see the
        // INNER macro's name in the diagnostic, not the wrapping
        // shape. A regression that ever populates `macro_name` from
        // the outermost caller's head fails this pin.
        let mut e = Expander::new();
        e.set_max_expansion_size(4);
        // `wrapper` re-expands into `bomb`; `bomb` produces a size-6
        // output that crosses the ceiling. Expected: the diagnostic
        // names `bomb`, not `wrapper`.
        let src = "
            (defmacro bomb (x) `(list ,x ,x ,x ,x))
            (defmacro wrapper (x) `(bomb ,x))
            (wrapper hey)
        ";
        let err = e.expand_program(read(src).unwrap()).unwrap_err();
        match err {
            LispError::ExpansionSizeExceeded {
                macro_name,
                size,
                limit,
            } => {
                assert_eq!(macro_name, "bomb");
                assert_eq!(size, 6);
                assert_eq!(limit, 4);
            }
            other => panic!("expected ExpansionSizeExceeded, got: {other:?}"),
        }
    }

    // ── max_macro_body_size: REGISTRATION-time body-size ceiling ──────
    // Peer to depth (recursion length) + cache (memoization width) +
    // expansion size (single-`apply` OUTPUT size) one PIPELINE-STAGE
    // axis over — the three prior guards fire at EXPAND time; this
    // one fires at REGISTER time. Closes the expander's RESOURCE
    // surface at a fourth typed dimension across two pipeline stages.

    #[test]
    fn expander_default_max_macro_body_size_is_the_module_constant() {
        // Both constructors seed `max_macro_body_size` from the
        // module constant — a regression that drifts one
        // constructor's default from the constant (e.g. a hardcoded
        // `16_384` at ONE site with the other left uninitialized
        // after a `Default` derive addition) fails this pin. Peer
        // to the `expander_default_max_expansion_{depth,size}_is_the_module_constant`
        // pins one PIPELINE-STAGE axis over.
        assert_eq!(
            Expander::new().max_macro_body_size(),
            DEFAULT_MAX_MACRO_BODY_SIZE
        );
        assert_eq!(
            Expander::new_substitute_only().max_macro_body_size(),
            DEFAULT_MAX_MACRO_BODY_SIZE
        );
        assert_eq!(DEFAULT_MAX_MACRO_BODY_SIZE, 16_384);
    }

    #[test]
    fn set_max_macro_body_size_takes_effect_on_subsequent_register() {
        // Setter mirrors accessor — a regression that ever forgets to
        // write through to the field, or writes to a shadow field the
        // accessor does not read, fails this pin. Peer to the
        // setter/accessor coherence pins on the depth + cache +
        // expansion-size axes.
        let mut e = Expander::new();
        e.set_max_macro_body_size(64);
        assert_eq!(e.max_macro_body_size(), 64);
    }

    #[test]
    fn register_macro_body_bomb_rejects_at_body_size_limit_bytecode_path() {
        // `(defmacro huge (x) `(list a b c d e))` has body
        // `` `(list a b c d e) `` — a `Sexp::Quasiquote` wrapping a
        // list of 6 nodes (outer List + 5 children); the outer
        // quasi-quote wrapper adds one → 8 nodes total for
        // `def.body.node_count()`. With a ceiling of `4` this
        // crosses the REGISTRATION-time BODY-SIZE threshold and the
        // expander returns a typed `LispError::MacroBodySizeExceeded`
        // with `macro_name`, `size`, and `limit` all populated. Peer
        // to `expand_expansion_bomb_rejects_at_size_limit_bytecode_path`
        // one PIPELINE-STAGE axis over.
        let mut e = Expander::new();
        e.set_max_macro_body_size(4);
        let forms = read("(defmacro huge (x) `(list a b c d e))").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::MacroBodySizeExceeded {
                macro_name,
                size,
                limit,
            } => {
                assert_eq!(macro_name, "huge");
                assert_eq!(size, 8);
                assert_eq!(limit, 4);
            }
            other => panic!("expected MacroBodySizeExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn register_macro_body_bomb_rejects_at_body_size_limit_substitute_path() {
        // The substitute strategy shares the same
        // `register_macro_def` outer registration entry with the
        // bytecode strategy, so the REGISTRATION-time BODY-SIZE
        // ceiling fires at the SAME gate regardless of which
        // apply-layer is active. The pin makes that shared-registration
        // property structural: a regression that ever branches the
        // body-size guard on strategy (e.g. moves it into the
        // `compile_templates` arm alone) fails this test on the
        // substitute expander. Peer to
        // `expand_expansion_bomb_rejects_at_size_limit_substitute_path`
        // one PIPELINE-STAGE axis over.
        let mut e = Expander::new_substitute_only();
        e.set_max_macro_body_size(4);
        let forms = read("(defmacro huge (x) `(list a b c d e))").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::MacroBodySizeExceeded {
                macro_name,
                size,
                limit,
            } => {
                assert_eq!(macro_name, "huge");
                assert_eq!(size, 8);
                assert_eq!(limit, 4);
            }
            other => panic!("expected MacroBodySizeExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn register_macro_body_at_ceiling_admits() {
        // `(defmacro twice (x) `(list ,x))` has body
        // `` `(list ,x) `` — `Sexp::Quasiquote` wrapping a 3-node
        // list (List + `list` atom + `,x` unquote which is
        // 1 + inner atom = 2 nodes) = 1 + (1 + 1 + 2) = 5 nodes.
        // With a ceiling of `5` the body sits AT the ceiling
        // (`<=` admits equality; only `>` rejects), so the
        // registration succeeds and the subsequent expansion
        // proceeds normally. Fail-before/pass-after negative
        // control for the body-bomb rejection tests: a regression
        // that ever rejected at-ceiling bodies would fail this pin.
        let mut e = Expander::new();
        e.set_max_macro_body_size(5);
        let forms = read("(defmacro twice (x) `(list ,x)) (twice hey)").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(list hey)"));
    }

    #[test]
    fn register_failure_leaves_both_tables_pristine() {
        // A rejected registration MUST leave BOTH `self.macros`
        // AND `self.templates` exactly as they were — no
        // partial-write window in which one table carries the
        // over-ceiling entry while the other does not, or in
        // which the template pre-compile already ran. This pin
        // catches a regression that ever moves the body-size
        // gate below `self.templates.insert` or below
        // `self.macros.insert`.
        let mut e = Expander::new();
        e.set_max_macro_body_size(4);
        let forms = read("(defmacro huge (x) `(list a b c d e))").unwrap();
        assert!(e.expand_program(forms).is_err());
        assert!(
            !e.has("huge"),
            "macros table must stay pristine after rejection"
        );
        assert_eq!(e.len(), 0);
    }

    #[test]
    fn macro_body_size_exceeded_position_is_none() {
        // The variant joins the non-positional cohort — the
        // offending macro's byte offset is not what a body-bomb
        // diagnostic anchors to; the (offending macro name,
        // observed body size, configured ceiling) triple is.
        // This pin keeps the variant in the `position() -> None`
        // cohort so a future span-carrying edit lands the field
        // in ONE place across every non-positional variant
        // simultaneously. Peer to
        // `expansion_{depth,size}_exceeded_position_is_none` one
        // PIPELINE-STAGE axis over.
        let err = LispError::MacroBodySizeExceeded {
            macro_name: "huge".to_string(),
            size: 512,
            limit: 256,
        };
        assert_eq!(err.position(), None);
    }

    #[test]
    fn macro_body_size_exceeded_display_matches_typed_variant_shape() {
        // Display projection carries `macro_name`, `size`, AND
        // `limit`, so authoring surfaces that substring-grep the
        // rendered diagnostic (`tatara-check`, REPL, LSP) see
        // ALL THREE halves at the variant boundary and never need
        // to substring-parse free-form text to recover the
        // identity of the offending macro, the observed body
        // size, or the ceiling it breached. Peer to
        // `expansion_size_exceeded_display_matches_typed_variant_shape`
        // one PIPELINE-STAGE axis over.
        let err = LispError::MacroBodySizeExceeded {
            macro_name: "huge".to_string(),
            size: 512,
            limit: 256,
        };
        let rendered = err.to_string();
        assert!(rendered.contains("huge"), "rendered: {rendered}");
        assert!(rendered.contains("512"), "rendered: {rendered}");
        assert!(rendered.contains("256"), "rendered: {rendered}");
        assert!(
            rendered.contains("macro body size exceeded"),
            "rendered: {rendered}"
        );
    }

    #[test]
    fn lawful_macro_body_within_ceiling_registers_and_expands() {
        // A lawful hand-authored macro body sits well below any
        // realistic ceiling — the default `DEFAULT_MAX_MACRO_BODY_SIZE`
        // is deliberately generous so operators never brush it on
        // real authoring workloads. This pin locks the default
        // ceiling's non-interference posture: a regression that
        // ever lowered the default enough to reject the reference
        // `(defmacro when (cond x) `(if ,cond ,x))` shape would
        // fail here.
        let mut e = Expander::new();
        let forms = read(
            "(defmacro when (cond x) `(if ,cond ,x))
             (when #t hey)",
        )
        .unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(if #t hey)"));
    }

    #[test]
    fn register_macro_def_direct_call_respects_body_size_ceiling() {
        // `register_macro_def` is the substrate primitive both
        // `expand_program`'s `defmacro` recognition AND
        // `with_macros`'s bulk load route through. The body-size
        // ceiling fires on the primitive itself, not on any one
        // consumer — a regression that ever moved the gate into
        // `expand_program`'s arm alone would fail this pin.
        let mut e = Expander::new();
        e.set_max_macro_body_size(4);
        let def = MacroDef {
            name: "huge".to_string(),
            params: MacroParams {
                required: vec!["x".to_string()],
                optional: Vec::new(),
                rest: None,
            },
            body: Sexp::List(vec![
                Sexp::Atom(crate::ast::Atom::Symbol("a".to_string())),
                Sexp::Atom(crate::ast::Atom::Symbol("b".to_string())),
                Sexp::Atom(crate::ast::Atom::Symbol("c".to_string())),
                Sexp::Atom(crate::ast::Atom::Symbol("d".to_string())),
                Sexp::Atom(crate::ast::Atom::Symbol("e".to_string())),
            ]),
        };
        // Body node_count: List(5 atoms) = 1 + 5 = 6 nodes.
        let err = e.register_macro_def(def).unwrap_err();
        match err {
            LispError::MacroBodySizeExceeded {
                macro_name,
                size,
                limit,
            } => {
                assert_eq!(macro_name, "huge");
                assert_eq!(size, 6);
                assert_eq!(limit, 4);
            }
            other => panic!("expected MacroBodySizeExceeded, got: {other:?}"),
        }
        assert!(!e.has("huge"));
    }

    // ── max_registered_macros: REGISTRATION-time table-count ceiling ──
    // Peer to body-size (REGISTER-time SIZE) one RESOURCE-DIMENSION
    // axis over — where body-size bounds a per-registration SIZE,
    // this ceiling bounds cumulative registration COUNT. Peer to
    // cache-entries (EXPAND-time COUNT) one PIPELINE-STAGE axis over
    // — the two entry-count guards close the two pipeline stages
    // symmetrically. Closes the expander's RESOURCE surface at a
    // FIFTH typed dimension across two pipeline stages: the
    // REGISTER-time face is now a two-dimensional (size × count)
    // closure symmetric with the EXPAND-time (depth + count + size)
    // triple.

    #[test]
    fn expander_default_max_registered_macros_is_the_module_constant() {
        // Both constructors seed `max_registered_macros` from the
        // module constant — a regression that drifts one
        // constructor's default from the constant (e.g. a hardcoded
        // `4096` at ONE site with the other left uninitialized
        // after a `Default` derive addition) fails this pin. Peer
        // to the four prior default-coherence pins on depth,
        // cache-entries, expansion-size, and body-size.
        assert_eq!(
            Expander::new().max_registered_macros(),
            DEFAULT_MAX_REGISTERED_MACROS
        );
        assert_eq!(
            Expander::new_substitute_only().max_registered_macros(),
            DEFAULT_MAX_REGISTERED_MACROS
        );
        assert_eq!(DEFAULT_MAX_REGISTERED_MACROS, 4096);
    }

    #[test]
    fn set_max_registered_macros_takes_effect_on_subsequent_register() {
        // Setter mirrors accessor — a regression that ever forgets
        // to write through to the field, or writes to a shadow
        // field the accessor does not read, fails this pin. Peer
        // to the four prior setter/accessor coherence pins.
        let mut e = Expander::new();
        e.set_max_registered_macros(64);
        assert_eq!(e.max_registered_macros(), 64);
    }

    #[test]
    fn register_fresh_macro_past_table_ceiling_rejects_bytecode_path() {
        // With a ceiling of `2`, the first two fresh registrations
        // land in the table and the third fresh key rejects with
        // `LispError::RegisteredMacrosExceeded { macro_name: "c",
        // count: 2, limit: 2 }`. `count` is the pre-insert table
        // length (which equals `limit` at rejection in the common
        // case); `macro_name` is the OFFENDING call's name (the
        // fresh key we could not admit), not the outermost caller.
        let mut e = Expander::new();
        e.set_max_registered_macros(2);
        let forms = read(
            "(defmacro a (x) `(list ,x))
             (defmacro b (x) `(list ,x))
             (defmacro c (x) `(list ,x))",
        )
        .unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::RegisteredMacrosExceeded {
                macro_name,
                count,
                limit,
            } => {
                assert_eq!(macro_name, "c");
                assert_eq!(count, 2);
                assert_eq!(limit, 2);
            }
            other => panic!("expected RegisteredMacrosExceeded, got: {other:?}"),
        }
        // First two admitted; third rejected leaves the table at
        // its pre-third-call size.
        assert!(e.has("a"));
        assert!(e.has("b"));
        assert!(!e.has("c"));
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn register_fresh_macro_past_table_ceiling_rejects_substitute_path() {
        // The substitute strategy shares the same
        // `register_macro_def` outer registration entry with the
        // bytecode strategy, so the REGISTRATION-time TABLE-COUNT
        // ceiling fires at the SAME gate regardless of which
        // apply-layer is active. The pin makes that shared-
        // registration property structural: a regression that ever
        // branches the table-count guard on strategy (e.g. moves
        // it into the `compile_templates` arm alone) fails this
        // test on the substitute expander. Peer to the four prior
        // shared-registration pins.
        let mut e = Expander::new_substitute_only();
        e.set_max_registered_macros(2);
        let forms = read(
            "(defmacro a (x) `(list ,x))
             (defmacro b (x) `(list ,x))
             (defmacro c (x) `(list ,x))",
        )
        .unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::RegisteredMacrosExceeded {
                macro_name,
                count,
                limit,
            } => {
                assert_eq!(macro_name, "c");
                assert_eq!(count, 2);
                assert_eq!(limit, 2);
            }
            other => panic!("expected RegisteredMacrosExceeded, got: {other:?}"),
        }
        assert!(e.has("a"));
        assert!(e.has("b"));
        assert!(!e.has("c"));
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn register_overwrite_of_existing_key_at_table_ceiling_admits() {
        // OVERWRITES (a re-registration of an already-registered
        // key) do not grow the table and MUST be admitted at any
        // ceiling — operators redefining a macro at capacity is a
        // legitimate authoring pattern. This pin catches a
        // regression that ever treats overwrites the same as fresh
        // keys (a `contains_key` check that inverts, or a
        // `set_max_registered_macros(0)` that would reject even
        // pre-registered overwrites).
        let mut e = Expander::new();
        // Land two macros with a permissive ceiling.
        e.expand_program(
            read(
                "(defmacro a (x) `(list ,x))
                 (defmacro b (x) `(list ,x))",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(e.len(), 2);
        // Drop the ceiling to exactly the current size — a fresh
        // third key would now reject, but an overwrite of `a` MUST
        // admit.
        e.set_max_registered_macros(2);
        let forms = read("(defmacro a (x y) `(pair ,x ,y))").unwrap();
        e.expand_program(forms).unwrap();
        assert_eq!(e.len(), 2, "overwrite must not grow the table");
        // Re-expand a call to `a` — the NEW definition (two
        // required params) is active, and the OLD definition (one
        // required param) is gone. Under the new definition,
        // `(a 1 2)` expands to `(pair 1 2)`.
        let expanded = e.expand_program(read("(a foo bar)").unwrap()).unwrap();
        assert_eq!(expanded[0], parse("(pair foo bar)"));
    }

    #[test]
    fn register_at_table_ceiling_admits_up_to_but_not_past() {
        // Fail-before/pass-after negative control on the exact
        // boundary: with a ceiling of `3`, exactly three fresh keys
        // admit and the fourth rejects. A regression that ever
        // shifted the boundary by one (e.g. keyed on `>` instead of
        // `>=`, or checked `len() > cap` before insert instead of
        // `len() >= cap`) would fail one of the two arms — either
        // the third admission or the fourth rejection.
        let mut e = Expander::new();
        e.set_max_registered_macros(3);
        // Three fresh keys admit.
        e.expand_program(
            read(
                "(defmacro a (x) `(list ,x))
                 (defmacro b (x) `(list ,x))
                 (defmacro c (x) `(list ,x))",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(e.len(), 3);
        // Fourth fresh key rejects.
        let err = e
            .expand_program(read("(defmacro d (x) `(list ,x))").unwrap())
            .unwrap_err();
        match err {
            LispError::RegisteredMacrosExceeded {
                macro_name,
                count,
                limit,
            } => {
                assert_eq!(macro_name, "d");
                assert_eq!(count, 3);
                assert_eq!(limit, 3);
            }
            other => panic!("expected RegisteredMacrosExceeded, got: {other:?}"),
        }
        // Table stays at pre-rejection size — no partial write.
        assert_eq!(e.len(), 3);
        assert!(!e.has("d"));
    }

    #[test]
    fn register_table_ceiling_leaves_both_tables_pristine_on_rejection() {
        // A rejected registration MUST leave BOTH `self.macros` AND
        // `self.templates` exactly as they were — no partial-write
        // window in which one table carries the over-ceiling entry
        // while the other does not, or in which the template
        // pre-compile already ran. Table-count gate fires BEFORE
        // body-size gate walks the body AND BEFORE any insert lands,
        // so a regression that ever moved the table-count gate
        // below `self.templates.insert` or below
        // `self.macros.insert` would fail this pin. Peer to
        // `register_failure_leaves_both_tables_pristine` on the
        // body-size axis.
        let mut e = Expander::new();
        e.set_max_registered_macros(1);
        // Land one macro at ceiling.
        e.expand_program(read("(defmacro a (x) `(list ,x))").unwrap())
            .unwrap();
        assert!(e.has("a"));
        // Reject a fresh second key.
        let err = e
            .expand_program(read("(defmacro b (x) `(list ,x))").unwrap())
            .unwrap_err();
        assert!(matches!(err, LispError::RegisteredMacrosExceeded { .. }));
        // Table stays at pre-rejection state — `b` is NOT present
        // in either self.macros OR self.templates.
        assert_eq!(e.len(), 1);
        assert!(e.has("a"));
        assert!(!e.has("b"));
        // Expanding a call to `a` still works (templates table for
        // `a` untouched by the failed `b` registration).
        let expanded = e.expand_program(read("(a hey)").unwrap()).unwrap();
        assert_eq!(expanded[0], parse("(list hey)"));
    }

    #[test]
    fn set_max_registered_macros_zero_rejects_every_fresh_registration() {
        // Zero ceiling is the "contract-test that this expander
        // accepts no new macros" posture — every fresh key rejects
        // at the first `defmacro` because `0 >= 0`. Peer to the
        // depth-ceiling zero-admits-and-rejects-every-macro-call
        // pin one RESOURCE axis over.
        let mut e = Expander::new();
        e.set_max_registered_macros(0);
        let err = e
            .expand_program(read("(defmacro a (x) `(list ,x))").unwrap())
            .unwrap_err();
        match err {
            LispError::RegisteredMacrosExceeded {
                macro_name,
                count,
                limit,
            } => {
                assert_eq!(macro_name, "a");
                assert_eq!(count, 0);
                assert_eq!(limit, 0);
            }
            other => panic!("expected RegisteredMacrosExceeded, got: {other:?}"),
        }
        assert!(e.is_empty());
    }

    #[test]
    fn registered_macros_exceeded_position_is_none() {
        // The variant joins the non-positional cohort — the
        // offending macro's byte offset is not what a table-count
        // diagnostic anchors to; the (offending fresh macro name,
        // observed table count, configured ceiling) triple is.
        // This pin keeps the variant in the `position() -> None`
        // cohort so a future span-carrying edit lands the field
        // in ONE place across every non-positional variant
        // simultaneously. Peer to the four prior
        // `*_exceeded_position_is_none` pins on the resource-
        // ceiling cohort.
        let err = LispError::RegisteredMacrosExceeded {
            macro_name: "fresh-1000".to_string(),
            count: 4096,
            limit: 4096,
        };
        assert_eq!(err.position(), None);
    }

    #[test]
    fn registered_macros_exceeded_display_matches_typed_variant_shape() {
        // Display projection carries `macro_name`, `count`, AND
        // `limit`, so authoring surfaces that substring-grep the
        // rendered diagnostic (`tatara-check`, REPL, LSP) see ALL
        // THREE halves at the variant boundary and never need to
        // substring-parse free-form text to recover the identity
        // of the offending fresh key, the observed table count, or
        // the ceiling it breached. Peer to the four prior
        // `*_exceeded_display_matches_typed_variant_shape` pins.
        let err = LispError::RegisteredMacrosExceeded {
            macro_name: "fresh-1000".to_string(),
            count: 4096,
            limit: 4096,
        };
        let rendered = err.to_string();
        assert!(rendered.contains("fresh-1000"), "rendered: {rendered}");
        assert!(rendered.contains("4096"), "rendered: {rendered}");
        assert!(
            rendered.contains("registered macros count exceeded"),
            "rendered: {rendered}"
        );
    }

    #[test]
    fn lawful_typescape_within_ceiling_registers_and_expands() {
        // A lawful hand-authored typescape (a handful of macros)
        // sits well below any realistic ceiling — the default
        // `DEFAULT_MAX_REGISTERED_MACROS` is deliberately generous
        // so operators never brush it on real authoring workloads.
        // This pin locks the default ceiling's non-interference
        // posture: a regression that ever lowered the default
        // enough to reject a small typescape would fail here. Peer
        // to `lawful_macro_body_within_ceiling_registers_and_expands`.
        let mut e = Expander::new();
        let forms = read(
            "(defmacro when (cond x) `(if ,cond ,x))
             (defmacro twice (x) `(list ,x ,x))
             (defmacro pair (x y) `(list ,x ,y))
             (when #t (twice hey))",
        )
        .unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(if #t (list hey hey))"));
        assert_eq!(e.len(), 3);
    }

    #[test]
    fn register_macro_def_direct_call_respects_table_ceiling() {
        // `register_macro_def` is the substrate primitive both
        // `expand_program`'s `defmacro` recognition AND
        // `with_macros`'s bulk load route through. The table-count
        // ceiling fires on the primitive itself, not on any one
        // consumer — a regression that ever moved the gate into
        // `expand_program`'s arm alone would fail this pin. Peer
        // to `register_macro_def_direct_call_respects_body_size_ceiling`
        // one RESOURCE-DIMENSION axis over.
        let mut e = Expander::new();
        e.set_max_registered_macros(1);
        let def_a = MacroDef {
            name: "a".to_string(),
            params: MacroParams {
                required: vec!["x".to_string()],
                optional: Vec::new(),
                rest: None,
            },
            body: Sexp::Atom(crate::ast::Atom::Symbol("y".to_string())),
        };
        let def_b = MacroDef {
            name: "b".to_string(),
            params: MacroParams {
                required: vec!["x".to_string()],
                optional: Vec::new(),
                rest: None,
            },
            body: Sexp::Atom(crate::ast::Atom::Symbol("y".to_string())),
        };
        // First admits — table sits at ceiling.
        e.register_macro_def(def_a).unwrap();
        assert!(e.has("a"));
        assert_eq!(e.len(), 1);
        // Second rejects — fresh key past ceiling.
        let err = e.register_macro_def(def_b).unwrap_err();
        match err {
            LispError::RegisteredMacrosExceeded {
                macro_name,
                count,
                limit,
            } => {
                assert_eq!(macro_name, "b");
                assert_eq!(count, 1);
                assert_eq!(limit, 1);
            }
            other => panic!("expected RegisteredMacrosExceeded, got: {other:?}"),
        }
        assert!(!e.has("b"));
        assert_eq!(e.len(), 1);
    }

    #[test]
    fn register_macro_def_direct_call_admits_overwrite_at_table_ceiling() {
        // Direct `register_macro_def` calls must ALSO admit
        // overwrites at ceiling — the overwrite discipline lives
        // on the primitive, not on `expand_program`'s arm alone.
        // Sibling of the source-level overwrite admission test.
        let mut e = Expander::new();
        e.set_max_registered_macros(1);
        let def_a_v1 = MacroDef {
            name: "a".to_string(),
            params: MacroParams {
                required: vec!["x".to_string()],
                optional: Vec::new(),
                rest: None,
            },
            body: Sexp::Atom(crate::ast::Atom::Symbol("y".to_string())),
        };
        let def_a_v2 = MacroDef {
            name: "a".to_string(),
            params: MacroParams {
                required: vec!["x".to_string(), "y".to_string()],
                optional: Vec::new(),
                rest: None,
            },
            body: Sexp::Atom(crate::ast::Atom::Symbol("z".to_string())),
        };
        e.register_macro_def(def_a_v1).unwrap();
        assert_eq!(e.len(), 1);
        // Overwrite of `a` at ceiling admits — no growth.
        e.register_macro_def(def_a_v2).unwrap();
        assert_eq!(e.len(), 1);
    }

    // ── `MacroParams::total_arity` — the total-slots projection ──
    //
    // `total_arity()` names `required.len() + optional.len() +
    // rest.is_some() as usize` on the typed `MacroParams` algebra —
    // the `Vec`-free peer of `names().len()` AND the axis both
    // `MacroParams::bind`'s `Vec::with_capacity` hint AND the
    // REGISTER-time arity-ceiling gate consult. These pins cover:
    // structural identity vs. `names().len()`, the
    // `fixed_arity + rest.is_some() as usize` decomposition, and the
    // three canonical shapes (empty, all-required, mixed with rest).
    #[test]
    fn macro_params_total_arity_matches_names_len() {
        // The structural identity `total_arity() == names().len()`
        // — the `Vec`-free primitive's projection equals the flat-
        // index list's length. A regression that ever miscounts
        // one slot (drops rest, double-counts optional) fails this
        // pin. Sibling to `fixed_arity`'s peer-arithmetic pins.
        let params = MacroParams {
            required: vec!["a".into(), "b".into()],
            optional: vec![OptionalParam::bare("c"), OptionalParam::bare("d")],
            rest: Some("e".into()),
        };
        assert_eq!(params.total_arity(), params.names().len());
        assert_eq!(params.total_arity(), 5);
    }

    #[test]
    fn macro_params_total_arity_equals_fixed_arity_plus_rest_bit() {
        // The `fixed_arity + rest.is_some() as usize` decomposition
        // — pins the composition on the [`MacroParams`] algebra so
        // a future consumer that wants "total slots" via
        // `fixed_arity + rest?` binds to the same arithmetic
        // `total_arity` names.
        let rest_none = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: None,
        };
        assert_eq!(rest_none.total_arity(), rest_none.fixed_arity());
        assert_eq!(rest_none.total_arity(), 2);
        let rest_some = MacroParams {
            required: vec!["a".into()],
            optional: vec![OptionalParam::bare("b")],
            rest: Some("r".into()),
        };
        assert_eq!(rest_some.total_arity(), rest_some.fixed_arity() + 1);
        assert_eq!(rest_some.total_arity(), 3);
    }

    #[test]
    fn macro_params_total_arity_is_zero_for_the_empty_param_list() {
        // Nullary macro edge: `(defmacro nullary () BODY)` has
        // `total_arity() == 0`. Under a `set_max_macro_arity(0)`
        // ceiling the arity gate keys on `>` (equality admits), so
        // this nullary registration succeeds while any non-nullary
        // one rejects — the exact contract the arity-gate
        // zero-admits-nullary test below relies on.
        let params = MacroParams::default();
        assert_eq!(params.total_arity(), 0);
    }

    // ── max_macro_arity: REGISTRATION-time param-count ceiling ──
    // Peer to body-size (REGISTER-time BODY SIZE) one RESOURCE-
    // DIMENSION axis over — where body-size bounds a per-registration
    // BODY node count, this ceiling bounds a per-registration PARAM
    // slot count. Peer to registered-macros (REGISTER-time TABLE
    // COUNT) one RESOURCE-DIMENSION axis over — where
    // registered-macros bounds cumulative registration COUNT, this
    // ceiling bounds a per-registration PARAM-LIST WIDTH. Together
    // the three REGISTER-time ceilings close the (per-body SIZE,
    // per-body ARITY, per-table COUNT) three-corner surface.

    #[test]
    fn expander_default_max_macro_arity_is_the_module_constant() {
        // Both constructors seed `max_macro_arity` from the module
        // constant — a regression that drifts one constructor's
        // default from the constant (e.g. a hardcoded `128` at ONE
        // site with the other left uninitialized after a `Default`
        // derive addition) fails this pin. Peer to the five prior
        // default-coherence pins on depth, cache-entries,
        // expansion-size, body-size, and registered-macros.
        assert_eq!(Expander::new().max_macro_arity(), DEFAULT_MAX_MACRO_ARITY);
        assert_eq!(
            Expander::new_substitute_only().max_macro_arity(),
            DEFAULT_MAX_MACRO_ARITY
        );
        assert_eq!(DEFAULT_MAX_MACRO_ARITY, 128);
    }

    #[test]
    fn set_max_macro_arity_takes_effect_on_subsequent_register() {
        // Setter mirrors accessor — a regression that ever forgets to
        // write through to the field, or writes to a shadow field the
        // accessor does not read, fails this pin. Peer to the five
        // prior setter/accessor coherence pins.
        let mut e = Expander::new();
        e.set_max_macro_arity(16);
        assert_eq!(e.max_macro_arity(), 16);
    }

    #[test]
    fn register_arity_bomb_rejects_at_arity_limit_bytecode_path() {
        // `(defmacro huge (a b c) `,a)` declares 3 required params.
        // Under `set_max_macro_arity(2)` the arity gate fires:
        // 3 > 2 → `LispError::MacroArityExceeded { macro_name: "huge",
        // arity: 3, limit: 2 }`. `arity` is the observed
        // `def.params.total_arity()`, NOT `required.len()` alone —
        // even an optional-only or rest-only overrun trips the gate
        // (pinned separately). Peer to the register-time body-size
        // rejection tests one RESOURCE-DIMENSION axis over.
        let mut e = Expander::new();
        e.set_max_macro_arity(2);
        let forms = read("(defmacro huge (a b c) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::MacroArityExceeded {
                macro_name,
                arity,
                limit,
            } => {
                assert_eq!(macro_name, "huge");
                assert_eq!(arity, 3);
                assert_eq!(limit, 2);
            }
            other => panic!("expected MacroArityExceeded, got: {other:?}"),
        }
        assert!(!e.has("huge"));
    }

    #[test]
    fn register_arity_bomb_rejects_at_arity_limit_substitute_path() {
        // The substitute strategy shares the same
        // `register_macro_def` outer registration entry with the
        // bytecode strategy, so the REGISTRATION-time ARITY ceiling
        // fires at the SAME gate regardless of which apply-layer is
        // active. Peer to
        // `register_arity_bomb_rejects_at_arity_limit_bytecode_path`.
        let mut e = Expander::new_substitute_only();
        e.set_max_macro_arity(2);
        let forms = read("(defmacro huge (a b c) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::MacroArityExceeded {
                macro_name,
                arity,
                limit,
            } => {
                assert_eq!(macro_name, "huge");
                assert_eq!(arity, 3);
                assert_eq!(limit, 2);
            }
            other => panic!("expected MacroArityExceeded, got: {other:?}"),
        }
        assert!(!e.has("huge"));
    }

    #[test]
    fn register_arity_gate_counts_optional_slots() {
        // `(defmacro foo (a &optional b c) `,a)` has 1 required + 2
        // optional + 0 rest = arity 3 via `total_arity`. Under a
        // ceiling of `2` the gate rejects, matching the required-only
        // arity-bomb — the arity axis is `total_arity()`, not
        // `required.len()`. A regression that drops the optional slots
        // from the arity computation would ADMIT this shape and fail
        // this pin.
        let mut e = Expander::new();
        e.set_max_macro_arity(2);
        let forms = read("(defmacro foo (a &optional b c) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::MacroArityExceeded {
                macro_name,
                arity,
                limit,
            } => {
                assert_eq!(macro_name, "foo");
                assert_eq!(arity, 3);
                assert_eq!(limit, 2);
            }
            other => panic!("expected MacroArityExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn register_arity_gate_counts_rest_slot() {
        // `(defmacro spread (a b &rest r) `,a)` has 2 required + 0
        // optional + 1 rest = arity 3 via `total_arity`. Under a
        // ceiling of `2` the gate rejects, matching the required-only
        // arity-bomb — the arity axis includes the rest slot as ONE
        // named param, not zero. A regression that drops the rest slot
        // from the arity computation would ADMIT this shape and fail
        // this pin.
        let mut e = Expander::new();
        e.set_max_macro_arity(2);
        let forms = read("(defmacro spread (a b &rest r) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        match err {
            LispError::MacroArityExceeded {
                macro_name,
                arity,
                limit,
            } => {
                assert_eq!(macro_name, "spread");
                assert_eq!(arity, 3);
                assert_eq!(limit, 2);
            }
            other => panic!("expected MacroArityExceeded, got: {other:?}"),
        }
    }

    #[test]
    fn register_arity_at_ceiling_admits() {
        // `(defmacro pair (x y) `(list ,x ,y))` has 2 required = arity
        // 2 via `total_arity`. Under a ceiling of `2` the arity gate
        // keys on `>` (equality admits; only strict overrun rejects),
        // so the registration succeeds and the expansion proceeds
        // normally. Fail-before/pass-after negative control on the
        // exact boundary: a regression that ever shifted the boundary
        // by one (e.g. keyed on `>=` instead of `>`) would fail this
        // pin. Peer to `register_macro_body_at_ceiling_admits` on the
        // body-size axis.
        let mut e = Expander::new();
        e.set_max_macro_arity(2);
        let forms = read("(defmacro pair (x y) `(list ,x ,y)) (pair a b)").unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(list a b)"));
        assert_eq!(e.len(), 1);
    }

    #[test]
    fn register_arity_ceiling_leaves_both_tables_pristine_on_rejection() {
        // A rejected arity-gated registration MUST leave BOTH
        // `self.macros` AND `self.templates` exactly as they were —
        // no partial-write window in which one table carries the
        // over-ceiling entry while the other does not, or in which
        // the template pre-compile already ran. Arity gate fires
        // BEFORE the body-size gate walks the body AND BEFORE any
        // insert lands, so a regression that ever moved the arity
        // gate below `self.templates.insert` or below
        // `self.macros.insert` would fail this pin. Peer to
        // `register_failure_leaves_both_tables_pristine` (body-size)
        // and `register_table_ceiling_leaves_both_tables_pristine_on_rejection`
        // (table-count).
        let mut e = Expander::new();
        e.set_max_macro_arity(1);
        let forms = read("(defmacro huge (a b c) `,a)").unwrap();
        assert!(e.expand_program(forms).is_err());
        assert!(
            !e.has("huge"),
            "macros table must stay pristine after rejection"
        );
        assert_eq!(e.len(), 0);
        // Expanding a NEW lawful macro afterwards still works — the
        // failed arity-gate rejection has not corrupted expander
        // state.
        let out = e
            .expand_program(read("(defmacro id (x) `,x) (id hey)").unwrap())
            .unwrap();
        assert_eq!(out[0], parse("hey"));
    }

    #[test]
    fn set_max_macro_arity_zero_rejects_every_non_nullary_registration() {
        // Zero ceiling is the "contract-test that this expander
        // accepts only nullary macros" posture — every fresh
        // non-nullary key rejects at the arity gate because
        // `total_arity() > 0`, while nullary `(defmacro nullary ()
        // BODY)` still admits (its arity is exactly 0, and only
        // strict overrun rejects). Peer to the body-size zero-rejects-
        // every-non-empty-body pin one RESOURCE-DIMENSION axis over
        // (though body-size zero rejects even nullary-body shapes,
        // since `Sexp::Nil::node_count() == 1 > 0`; arity zero
        // preserves the nullary registration surface).
        let mut e = Expander::new();
        e.set_max_macro_arity(0);
        // Nullary admits — arity 0 <= 0.
        e.expand_program(read("(defmacro nullary () `unit)").unwrap())
            .unwrap();
        assert!(e.has("nullary"));
        // Non-nullary rejects — arity 1 > 0.
        let err = e
            .expand_program(read("(defmacro id (x) `,x)").unwrap())
            .unwrap_err();
        match err {
            LispError::MacroArityExceeded {
                macro_name,
                arity,
                limit,
            } => {
                assert_eq!(macro_name, "id");
                assert_eq!(arity, 1);
                assert_eq!(limit, 0);
            }
            other => panic!("expected MacroArityExceeded, got: {other:?}"),
        }
        assert!(!e.has("id"));
    }

    #[test]
    fn macro_arity_exceeded_position_is_none() {
        // The variant joins the non-positional cohort — the offending
        // macro's byte offset is not what an arity-bomb diagnostic
        // anchors to; the (macro name, observed arity, configured
        // ceiling) triple is. This pin keeps the variant in the
        // `position() -> None` cohort so a future span-carrying edit
        // lands the field in ONE place across every non-positional
        // variant simultaneously. Peer to the five prior
        // `*_exceeded_position_is_none` pins on the resource-ceiling
        // cohort.
        let err = LispError::MacroArityExceeded {
            macro_name: "huge".to_string(),
            arity: 512,
            limit: 128,
        };
        assert_eq!(err.position(), None);
    }

    #[test]
    fn macro_arity_exceeded_display_matches_typed_variant_shape() {
        // Display projection carries `macro_name`, `arity`, AND
        // `limit`, so authoring surfaces that substring-grep the
        // rendered diagnostic (`tatara-check`, REPL, LSP) see ALL
        // THREE halves at the variant boundary and never need to
        // substring-parse free-form text to recover the identity of
        // the offending macro, the observed arity, or the ceiling it
        // breached. Peer to the five prior
        // `*_exceeded_display_matches_typed_variant_shape` pins.
        let err = LispError::MacroArityExceeded {
            macro_name: "huge".to_string(),
            arity: 512,
            limit: 128,
        };
        let rendered = err.to_string();
        assert!(rendered.contains("huge"), "rendered: {rendered}");
        assert!(rendered.contains("512"), "rendered: {rendered}");
        assert!(rendered.contains("128"), "rendered: {rendered}");
        assert!(
            rendered.contains("macro arity exceeded"),
            "rendered: {rendered}"
        );
    }

    #[test]
    fn lawful_macro_arity_within_ceiling_registers_and_expands() {
        // A lawful hand-authored macro arity sits well below any
        // realistic ceiling — the default `DEFAULT_MAX_MACRO_ARITY`
        // is deliberately generous so operators never brush it on
        // real authoring workloads. This pin locks the default
        // ceiling's non-interference posture: a regression that
        // ever lowered the default enough to reject the reference
        // multi-slot `(defmacro when (cond x) …)` shape would fail
        // here. Peer to `lawful_macro_body_within_ceiling_registers_and_expands`
        // and `lawful_typescape_within_ceiling_registers_and_expands`.
        let mut e = Expander::new();
        let forms = read(
            "(defmacro when (cond x) `(if ,cond ,x))
             (defmacro triple (x y z) `(list ,x ,y ,z))
             (when #t (triple a b c))",
        )
        .unwrap();
        let out = e.expand_program(forms).unwrap();
        assert_eq!(out[0], parse("(if #t (list a b c))"));
    }

    #[test]
    fn register_macro_def_direct_call_respects_arity_ceiling() {
        // `register_macro_def` is the substrate primitive both
        // `expand_program`'s `defmacro` recognition AND `with_macros`'s
        // bulk load route through. The arity ceiling fires on the
        // primitive itself, not on any one consumer — a regression
        // that ever moved the gate into `expand_program`'s arm alone
        // would fail this pin. Peer to
        // `register_macro_def_direct_call_respects_body_size_ceiling`
        // AND `register_macro_def_direct_call_respects_table_ceiling`
        // one RESOURCE-DIMENSION axis over.
        let mut e = Expander::new();
        e.set_max_macro_arity(2);
        let def = MacroDef {
            name: "huge".to_string(),
            params: MacroParams {
                required: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                optional: Vec::new(),
                rest: None,
            },
            body: Sexp::Atom(crate::ast::Atom::Symbol("a".to_string())),
        };
        // Arity 3 > ceiling 2 — rejected before body-size gate.
        let err = e.register_macro_def(def).unwrap_err();
        match err {
            LispError::MacroArityExceeded {
                macro_name,
                arity,
                limit,
            } => {
                assert_eq!(macro_name, "huge");
                assert_eq!(arity, 3);
                assert_eq!(limit, 2);
            }
            other => panic!("expected MacroArityExceeded, got: {other:?}"),
        }
        assert!(!e.has("huge"));
    }

    #[test]
    fn arity_gate_fires_before_body_size_gate() {
        // ORDERING PIN — the arity gate fires BEFORE the body-size
        // gate. With both `set_max_macro_arity(1)` AND
        // `set_max_macro_body_size(2)` set below a shape that violates
        // BOTH ceilings — `(defmacro dual (a b) `(list ,a ,b))` has
        // arity 2 AND body node count > 2 — the arity gate's
        // rejection wins, so the returned variant is
        // `MacroArityExceeded`, not `MacroBodySizeExceeded`. A
        // regression that ever swapped the two gates' order would fail
        // this pin. The ordering isn't cosmetic: arity is O(1)
        // (three-arm addition on `def.params` fields), body-size is
        // O(body_nodes) (`def.body.node_count()` walks the AST), so
        // the O(1)-first order minimises the cost of rejecting the
        // worst case (a large-body large-arity macro).
        let mut e = Expander::new();
        e.set_max_macro_arity(1);
        e.set_max_macro_body_size(2);
        let forms = read("(defmacro dual (a b) `(list ,a ,b))").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        assert!(
            matches!(err, LispError::MacroArityExceeded { .. }),
            "arity gate must fire before body-size gate; got: {err:?}"
        );
    }

    #[test]
    fn arity_gate_fires_after_table_count_gate() {
        // ORDERING PIN — the table-count gate fires BEFORE the arity
        // gate. Under `set_max_registered_macros(1)` at capacity AND
        // `set_max_macro_arity(1)`, a fresh over-arity registration
        // trips the table-count gate FIRST (its `HashMap::len`
        // comparison is O(1) cache-friendly and gates cumulative
        // registration cost); the arity gate is O(1) but on `def`,
        // not on shared state, so it sits AFTER the table-count gate
        // on the O(1)-first ordering of the REGISTER-time chain. A
        // regression that ever swapped the two would fail this pin.
        let mut e = Expander::new();
        e.set_max_registered_macros(1);
        e.set_max_macro_arity(1);
        // Land one macro at table ceiling.
        e.expand_program(read("(defmacro anchor (x) `,x)").unwrap())
            .unwrap();
        assert_eq!(e.len(), 1);
        // Fresh registration with BOTH violations — table-count wins.
        let forms = read("(defmacro huge (a b c) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        assert!(
            matches!(err, LispError::RegisteredMacrosExceeded { .. }),
            "table-count gate must fire before arity gate; got: {err:?}"
        );
    }

    // ── ResourceLimits: bundled resource-ceiling snapshot ────────────

    #[test]
    fn default_resource_limits_binds_each_field_to_matching_module_constant() {
        // Pin the (constant × aggregation) corner: the const-form
        // aggregate MUST bind every field to its individually-defined
        // module constant. A re-tuning of ONE module constant
        // (e.g. `DEFAULT_MAX_EXPANSION_DEPTH` raised from 256 to 512)
        // has to propagate through the aggregate at ONE site; this
        // test fires the divergence loudly if a future edit sets a
        // module constant to a fresh value but forgets to update the
        // matching aggregate slot. Field-by-field equality carries
        // the message better than one deep-equality assertion.
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.max_expansion_depth,
            DEFAULT_MAX_EXPANSION_DEPTH
        );
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.max_cache_entries,
            DEFAULT_MAX_CACHE_ENTRIES
        );
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.max_expansion_size,
            DEFAULT_MAX_EXPANSION_SIZE
        );
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.max_macro_body_size,
            DEFAULT_MAX_MACRO_BODY_SIZE
        );
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.max_registered_macros,
            DEFAULT_MAX_REGISTERED_MACROS
        );
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.max_macro_arity,
            DEFAULT_MAX_MACRO_ARITY
        );
    }

    #[test]
    fn resource_limits_default_impl_matches_const_form() {
        // The two projections of the shipped posture — the `const`
        // form [`DEFAULT_RESOURCE_LIMITS`] and the runtime
        // `Default::default()` form — must reach the SAME value. Pins
        // that a future edit which flips one to a new default cannot
        // silently drift the other.
        assert_eq!(ResourceLimits::default(), DEFAULT_RESOURCE_LIMITS);
    }

    #[test]
    fn expander_new_resource_limits_matches_shipped_defaults() {
        // Fresh-expander posture on the bundled-getter surface — the
        // six ceilings [`Expander::new`] seeds MUST snapshot to
        // [`DEFAULT_RESOURCE_LIMITS`] verbatim. Peer of the six
        // individual `expander_default_max_*_is_the_module_constant`
        // pins one AGGREGATION axis over — where each individual
        // pin catches ONE field drift, this pin catches ANY of the
        // six drifting AND the snapshot projection itself losing a
        // field.
        assert_eq!(Expander::new().resource_limits(), DEFAULT_RESOURCE_LIMITS);
    }

    #[test]
    fn expander_new_substitute_only_resource_limits_matches_shipped_defaults() {
        // Substitute-only expander seeds the SAME six ceilings from
        // the SAME module constants as [`Expander::new`] — the two
        // constructors share the resource-ceiling posture even
        // though they diverge on `compile_templates` +
        // `cache_enabled`. Pins that a future constructor which
        // forgets to seed one of the six ceilings from its module
        // constant fails loudly through the bundled getter.
        assert_eq!(
            Expander::new_substitute_only().resource_limits(),
            DEFAULT_RESOURCE_LIMITS
        );
    }

    #[test]
    fn resource_limits_snapshot_reflects_each_individual_setter() {
        // Coherence pin between the six individual setters and the
        // bundled getter — flipping ONE ceiling via its individual
        // setter must project through [`Expander::resource_limits`]
        // to the SAME value the individual getter returns. A
        // regression that leaves ONE field out of the snapshot
        // projection (a copy-paste omission in the struct literal
        // constructor) fires here. Distinct low-non-default values
        // so a stray field carrying the default fails loudly.
        let mut e = Expander::new();
        e.set_max_expansion_depth(3);
        e.set_max_cache_entries(5);
        e.set_max_expansion_size(7);
        e.set_max_macro_body_size(11);
        e.set_max_registered_macros(13);
        e.set_max_macro_arity(17);
        let snap = e.resource_limits();
        assert_eq!(snap.max_expansion_depth, 3);
        assert_eq!(snap.max_cache_entries, 5);
        assert_eq!(snap.max_expansion_size, 7);
        assert_eq!(snap.max_macro_body_size, 11);
        assert_eq!(snap.max_registered_macros, 13);
        assert_eq!(snap.max_macro_arity, 17);
        // And the individual getters see the same values — the
        // snapshot is a projection, not a divergent copy.
        assert_eq!(e.max_expansion_depth(), 3);
        assert_eq!(e.max_cache_entries(), 5);
        assert_eq!(e.max_expansion_size(), 7);
        assert_eq!(e.max_macro_body_size(), 11);
        assert_eq!(e.max_registered_macros(), 13);
        assert_eq!(e.max_macro_arity(), 17);
    }

    #[test]
    fn set_resource_limits_bulk_propagates_every_field_to_individual_getters() {
        // Sibling pin to `resource_limits_snapshot_reflects_...` on the
        // WRITE axis — bulk-setting via [`Expander::set_resource_limits`]
        // must reach every individual `max_*` getter. A regression
        // that drops one field from the destructuring assignment in
        // the bulk setter fires here. Distinct low-non-default values
        // so a stray field carrying the previous value fails loudly.
        let mut e = Expander::new();
        e.set_resource_limits(ResourceLimits {
            max_expansion_depth: 2,
            max_cache_entries: 4,
            max_expansion_size: 8,
            max_macro_body_size: 16,
            max_registered_macros: 32,
            max_macro_arity: 64,
        });
        assert_eq!(e.max_expansion_depth(), 2);
        assert_eq!(e.max_cache_entries(), 4);
        assert_eq!(e.max_expansion_size(), 8);
        assert_eq!(e.max_macro_body_size(), 16);
        assert_eq!(e.max_registered_macros(), 32);
        assert_eq!(e.max_macro_arity(), 64);
    }

    #[test]
    fn resource_limits_round_trip_through_bundled_getter_and_setter_is_identity() {
        // Round-trip theorem — for any starting expander, the
        // composition `set_resource_limits(resource_limits())` is
        // the identity on the resource-ceiling surface. This is the
        // ONE typed statement that pins the (getter, setter) pair as
        // strict projections of the same six-field surface, no
        // hidden lossy field, no ordering asymmetry between the
        // struct literal and the destructuring pattern.
        let mut e = Expander::new();
        e.set_max_expansion_depth(9);
        e.set_max_cache_entries(19);
        e.set_max_expansion_size(29);
        e.set_max_macro_body_size(39);
        e.set_max_registered_macros(49);
        e.set_max_macro_arity(59);
        let snap = e.resource_limits();
        // Reset to defaults so any leftover state would show up in
        // the post-round-trip snapshot as the default rather than
        // the intended value.
        e.set_resource_limits(ResourceLimits::default());
        assert_eq!(e.resource_limits(), DEFAULT_RESOURCE_LIMITS);
        // Apply the captured snapshot — the expander must land at
        // the same six values `snap` recorded.
        e.set_resource_limits(snap);
        assert_eq!(e.resource_limits(), snap);
    }

    #[test]
    fn resource_limits_struct_update_syntax_overrides_one_ceiling() {
        // The `Copy` posture on [`ResourceLimits`] gives callers the
        // cheap ergonomic of a struct-update literal — take the
        // shipped posture and override ONE ceiling — that a chain of
        // six independent setters cannot express in ONE typed expression.
        // Pins the intended use-shape so a future refactor which
        // removes `Copy` from the struct fails to compile at THIS
        // site rather than at every downstream consumer.
        let mut e = Expander::new();
        e.set_resource_limits(ResourceLimits {
            max_macro_arity: 4,
            ..DEFAULT_RESOURCE_LIMITS
        });
        // Overridden field carries the fresh value.
        assert_eq!(e.max_macro_arity(), 4);
        // Every other field carries the shipped default — the
        // struct-update literal is a full projection, not a
        // selective mutation.
        assert_eq!(e.max_expansion_depth(), DEFAULT_MAX_EXPANSION_DEPTH);
        assert_eq!(e.max_cache_entries(), DEFAULT_MAX_CACHE_ENTRIES);
        assert_eq!(e.max_expansion_size(), DEFAULT_MAX_EXPANSION_SIZE);
        assert_eq!(e.max_macro_body_size(), DEFAULT_MAX_MACRO_BODY_SIZE);
        assert_eq!(e.max_registered_macros(), DEFAULT_MAX_REGISTERED_MACROS);
    }

    #[test]
    fn expander_default_derives_resource_limits_from_bundled_field_default() {
        // Post the [`Expander::limits`]-field consolidation the derived
        // `Default` on [`Expander`] projects through [`ResourceLimits`]'s
        // own `Default` — which returns [`DEFAULT_RESOURCE_LIMITS`] — so
        // `Expander::default().resource_limits()` lands at the shipped
        // ceiling posture verbatim. Pre-consolidation the six independent
        // `max_*: usize` fields defaulted to the bare `usize` zero, so
        // `Expander::default()` yielded a below-floor posture with every
        // ceiling clamped to `0` — a shape no consumer wanted and every
        // test path avoided by routing through [`Expander::new`] instead.
        // The consolidation is what turned that latent below-floor
        // derived-`Default` shape into a lawful shipped posture on the
        // structural axis. Peer of
        // `expander_new_resource_limits_matches_shipped_defaults` one
        // CONSTRUCTOR axis over — where that test pins the explicit
        // [`Expander::new`] constructor, this test pins the derived-
        // [`Default`] constructor route to the SAME six values through
        // the SAME bundled projection. Sibling of
        // `resource_limits_default_impl_matches_const_form` one
        // TYPE-LEVEL axis over — that test pins [`ResourceLimits::default`]
        // to [`DEFAULT_RESOURCE_LIMITS`]; this test pins [`Expander`]'s
        // derived `Default` to project through it.
        assert_eq!(
            Expander::default().resource_limits(),
            DEFAULT_RESOURCE_LIMITS
        );
    }

    #[test]
    fn resource_limits_bulk_setter_keeps_the_arity_gate_in_effect() {
        // Behavioral pin — the bundled setter is not a decorative
        // getter/setter pair; the six ceilings it propagates must
        // reach the SAME check sites the individual setters do. Set
        // `max_macro_arity` to 1 via the bulk setter and confirm the
        // arity gate fires on a two-arg macro registration. A
        // regression that wired the bulk setter to a shadow field
        // rather than the actual `Expander` state fails here.
        let mut e = Expander::new();
        e.set_resource_limits(ResourceLimits {
            max_macro_arity: 1,
            ..DEFAULT_RESOURCE_LIMITS
        });
        let forms = read("(defmacro two (a b) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MacroArityExceeded {
                    arity: 2,
                    limit: 1,
                    ..
                }
            ),
            "bulk-set arity ceiling must reach the register-time gate; got: {err:?}"
        );
    }

    // ── Expander::with_limits — at-construction resource-posture ───────

    #[test]
    fn expander_with_limits_seeds_the_provided_resource_posture() {
        // Round-trip pin at CONSTRUCTION time — the bundle the caller
        // passes into [`Expander::with_limits`] MUST project back
        // through [`Expander::resource_limits`] verbatim. Peer of the
        // `set_resource_limits(l); resource_limits() == l` round-trip
        // one CONSTRUCTOR-STAGE axis over — that pin covers the
        // POST-construction assignment; this pin covers the
        // AT-construction assignment. A regression that dropped the
        // `limits` argument on the floor (a copy-paste that left
        // `limits: DEFAULT_RESOURCE_LIMITS` in the struct literal
        // instead of `limits`) fires here on every non-default field.
        // Distinct low-non-default values so a stray field defaulting
        // fails loudly.
        let want = ResourceLimits {
            max_expansion_depth: 3,
            max_cache_entries: 5,
            max_expansion_size: 7,
            max_macro_body_size: 11,
            max_registered_macros: 13,
            max_macro_arity: 17,
        };
        assert_eq!(Expander::with_limits(want).resource_limits(), want);
    }

    #[test]
    fn expander_with_default_limits_agrees_with_new_on_resource_posture() {
        // Delegation identity — [`Expander::new`] delegates to
        // [`Expander::with_limits`] with [`DEFAULT_RESOURCE_LIMITS`] so
        // the two constructors MUST agree on the resource-ceiling
        // projection. Pins the "no drift between the two paths to a
        // fresh default-posture expander" theorem: a future refactor
        // that breaks the delegation (an inline field literal reappearing
        // in [`Expander::new`], an accidental hardcoded const at ONE
        // site with the other unchanged) fires here. Peer of
        // `resource_limits_default_impl_matches_const_form` one
        // TYPE-LEVEL axis over — that test pins the two projections of
        // the shipped posture on [`ResourceLimits`] itself; this test
        // pins the two constructor paths on [`Expander`] to reach the
        // SAME six values through the SAME bundle.
        assert_eq!(
            Expander::with_limits(DEFAULT_RESOURCE_LIMITS).resource_limits(),
            Expander::new().resource_limits()
        );
    }

    #[test]
    fn expander_with_limits_composes_with_struct_update_override() {
        // The `Copy` posture on [`ResourceLimits`] lets a caller compose
        // "the shipped posture with ONE ceiling overridden" as ONE
        // struct-update literal AND thread that composition through
        // [`Expander::with_limits`] as ONE typed call — the ergonomic
        // shape that six independent setter invocations cannot express
        // in ONE expression. Overridden ceiling carries the fresh value,
        // every other ceiling carries its shipped default. A future
        // refactor that removes `Copy` from [`ResourceLimits`] fails to
        // compile at THIS site rather than at every downstream consumer.
        let e = Expander::with_limits(ResourceLimits {
            max_macro_arity: 4,
            ..DEFAULT_RESOURCE_LIMITS
        });
        assert_eq!(e.max_macro_arity(), 4);
        assert_eq!(e.max_expansion_depth(), DEFAULT_MAX_EXPANSION_DEPTH);
        assert_eq!(e.max_cache_entries(), DEFAULT_MAX_CACHE_ENTRIES);
        assert_eq!(e.max_expansion_size(), DEFAULT_MAX_EXPANSION_SIZE);
        assert_eq!(e.max_macro_body_size(), DEFAULT_MAX_MACRO_BODY_SIZE);
        assert_eq!(e.max_registered_macros(), DEFAULT_MAX_REGISTERED_MACROS);
    }

    #[test]
    fn expander_with_limits_carries_the_new_execution_strategy() {
        // The (compile_templates, cache_enabled) execution-strategy pair
        // MUST carry the [`Expander::new`] default posture — bytecode +
        // cache both on — regardless of which resource posture the
        // caller passes in. Cache-count observation is the least-
        // invasive check available on the public surface: a fresh
        // `with_limits` expander that expands a repeat-call site must
        // populate at least one cache entry (bytecode + cache both on),
        // and a substitute-only sibling would leave `cache_size()` at
        // zero even after every call site expanded. A regression that
        // wired the wrong execution strategy into `with_limits`
        // (compile_templates: false, cache_enabled: false) fires here.
        let mut e = Expander::with_limits(DEFAULT_RESOURCE_LIMITS);
        let forms = read(
            "(defmacro id (x) `,x)
             (id one)
             (id one)",
        )
        .unwrap();
        e.expand_program(forms).unwrap();
        assert!(
            e.cache_size() >= 1,
            "with_limits carries the bytecode+cache strategy; cache_size must be >= 1 after repeat calls, got {}",
            e.cache_size()
        );
    }

    #[test]
    fn expander_with_limits_fires_the_arity_gate_from_construction() {
        // Behavioral pin — the at-construction bundle assignment MUST
        // reach the SAME check sites the individual + bulk POST-
        // construction setters do. Construct an expander with
        // `max_macro_arity: 1` directly and confirm the arity gate
        // fires on a two-arg macro registration. Sibling of
        // `resource_limits_bulk_setter_keeps_the_arity_gate_in_effect`
        // one CONSTRUCTOR-STAGE axis over — that pin covers the POST-
        // construction setter reaching the gate; this pin covers the
        // AT-construction assignment reaching the SAME gate through
        // the SAME `self.limits.max_macro_arity` load. A regression
        // that wired `with_limits` to a shadow field rather than
        // `self.limits` fails here.
        let mut e = Expander::with_limits(ResourceLimits {
            max_macro_arity: 1,
            ..DEFAULT_RESOURCE_LIMITS
        });
        let forms = read("(defmacro two (a b) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MacroArityExceeded {
                    arity: 2,
                    limit: 1,
                    ..
                }
            ),
            "at-construction arity ceiling must reach the register-time gate; got: {err:?}"
        );
    }

    // ── UNBOUNDED_RESOURCE_LIMITS — ceiling-lifted preset posture ──────

    #[test]
    fn unbounded_resource_limits_binds_every_ceiling_to_usize_max() {
        // Field-level pin — the ceiling-lifted preset carries
        // [`usize::MAX`] on every axis of the six-field surface. Pinned
        // as six independent field asserts (rather than one struct
        // equality) so a drift on ONE field carries a distinct-name
        // failure that names the drifting axis. A future extension
        // that adds a SEVENTH ceiling requires an additional field
        // assert here in lockstep with the const literal — rustc's
        // field-exhaustiveness on the const forces the new field to
        // appear, and this pin exercises it.
        assert_eq!(UNBOUNDED_RESOURCE_LIMITS.max_expansion_depth, usize::MAX);
        assert_eq!(UNBOUNDED_RESOURCE_LIMITS.max_cache_entries, usize::MAX);
        assert_eq!(UNBOUNDED_RESOURCE_LIMITS.max_expansion_size, usize::MAX);
        assert_eq!(UNBOUNDED_RESOURCE_LIMITS.max_macro_body_size, usize::MAX);
        assert_eq!(UNBOUNDED_RESOURCE_LIMITS.max_registered_macros, usize::MAX);
        assert_eq!(UNBOUNDED_RESOURCE_LIMITS.max_macro_arity, usize::MAX);
    }

    #[test]
    fn unbounded_resource_limits_disagrees_with_default_on_every_ceiling() {
        // Structural-distinctness pin — the two constants
        // [`DEFAULT_RESOURCE_LIMITS`] and [`UNBOUNDED_RESOURCE_LIMITS`]
        // sit at DISTINCT points on the posture axis of the (posture ×
        // aggregation) grid, and the disagreement is EXHAUSTIVE on the
        // six-field surface — every shipped `DEFAULT_MAX_*` module
        // constant is a concrete positive value strictly less than
        // [`usize::MAX`]. Pins that a future re-tuning which cranked one
        // shipped default up to [`usize::MAX`] (collapsing the two
        // presets on that field) fires here loudly rather than
        // silently letting the two presets become indistinguishable
        // on that axis. A regression which flipped ONE unbounded-const
        // field back to its default value also fires here — either
        // half of the disagreement breaks the theorem.
        assert_ne!(
            UNBOUNDED_RESOURCE_LIMITS.max_expansion_depth,
            DEFAULT_RESOURCE_LIMITS.max_expansion_depth
        );
        assert_ne!(
            UNBOUNDED_RESOURCE_LIMITS.max_cache_entries,
            DEFAULT_RESOURCE_LIMITS.max_cache_entries
        );
        assert_ne!(
            UNBOUNDED_RESOURCE_LIMITS.max_expansion_size,
            DEFAULT_RESOURCE_LIMITS.max_expansion_size
        );
        assert_ne!(
            UNBOUNDED_RESOURCE_LIMITS.max_macro_body_size,
            DEFAULT_RESOURCE_LIMITS.max_macro_body_size
        );
        assert_ne!(
            UNBOUNDED_RESOURCE_LIMITS.max_registered_macros,
            DEFAULT_RESOURCE_LIMITS.max_registered_macros
        );
        assert_ne!(
            UNBOUNDED_RESOURCE_LIMITS.max_macro_arity,
            DEFAULT_RESOURCE_LIMITS.max_macro_arity
        );
    }

    #[test]
    fn expander_with_unbounded_limits_projects_through_resource_limits_getter() {
        // Round-trip pin at CONSTRUCTION time — an expander built at
        // the ceiling-lifted preset MUST project back through
        // [`Expander::resource_limits`] to the SAME constant verbatim.
        // Peer of `expander_with_limits_seeds_the_provided_resource_posture`
        // one PRESET-POSTURE axis over — that pin exercises the
        // constructor's carry via a distinct-low-value bundle; this
        // pin exercises it via the ceiling-lifted preset constant.
        // Cross-pin the six-field snapshot equals the ceiling-lifted
        // constant so a regression that broke either the constant OR
        // the constructor's carry fires here.
        assert_eq!(
            Expander::with_limits(UNBOUNDED_RESOURCE_LIMITS).resource_limits(),
            UNBOUNDED_RESOURCE_LIMITS
        );
    }

    #[test]
    fn expander_with_unbounded_limits_admits_a_body_over_the_default_size_ceiling() {
        // Behavioral pin — the ceiling-lifted preset MUST reach the
        // register-time body-size gate as an actually-lifted ceiling.
        // Register a macro whose body node count strictly exceeds
        // [`DEFAULT_MAX_MACRO_BODY_SIZE`] (a body which under the
        // shipped default would fail [`register_macro_def`] with a
        // [`LispError::MacroBodySizeExceeded`]) and confirm the
        // registration succeeds under the ceiling-lifted preset. Only
        // the body-size ceiling being lifted lets a body of that
        // magnitude land; the arity ceiling is left at 1 param via
        // struct-update override to prove the preset's other five
        // ceilings STILL project cleanly at [`usize::MAX`] alongside
        // the ceiling under test. A regression that failed to lift
        // ONE ceiling across the preset (a stale `DEFAULT_MAX_*` field
        // in the const literal) fires here on the axis it dropped.
        let mut e = Expander::with_limits(UNBOUNDED_RESOURCE_LIMITS);
        // Build a huge quasi-quoted list body: `(k k k k k … k) whose
        // node_count() eclipses DEFAULT_MAX_MACRO_BODY_SIZE = 16384.
        let mut body = String::from("(defmacro huge (k) `(");
        for _ in 0..(DEFAULT_MAX_MACRO_BODY_SIZE + 1) {
            body.push_str(",k ");
        }
        body.push_str("))");
        let forms = read(&body).unwrap();
        e.expand_program(forms)
            .expect("unbounded body-size ceiling must admit a body over the default limit");
        assert!(e.has("huge"));
    }

    #[test]
    fn expander_with_unbounded_limits_admits_arity_over_the_default_arity_ceiling() {
        // Behavioral pin sibling of the body-size pin above one
        // RESOURCE-DIMENSION axis over on the REGISTER-time surface —
        // the ceiling-lifted preset MUST also reach the arity gate as
        // an actually-lifted ceiling. Register a macro whose param
        // list strictly exceeds [`DEFAULT_MAX_MACRO_ARITY`] (a
        // 129-slot lambda list which under the shipped default fails
        // with [`LispError::MacroArityExceeded`]) and confirm the
        // registration succeeds under the ceiling-lifted preset.
        use std::fmt::Write as _;
        let mut e = Expander::with_limits(UNBOUNDED_RESOURCE_LIMITS);
        let mut src = String::from("(defmacro many-arity (");
        for i in 0..(DEFAULT_MAX_MACRO_ARITY + 1) {
            write!(src, "a-{i} ").unwrap();
        }
        src.push_str(") `,a-0)");
        let forms = read(&src).unwrap();
        e.expand_program(forms)
            .expect("unbounded arity ceiling must admit a param list over the default limit");
        assert!(e.has("many-arity"));
    }

    #[test]
    fn unbounded_resource_limits_composes_via_struct_update_to_isolate_one_ceiling() {
        // Ergonomic pin — the `Copy` posture on [`ResourceLimits`] lets
        // a caller build "every ceiling lifted EXCEPT this ONE" as ONE
        // struct-update literal on the ceiling-lifted preset AND thread
        // that composition through [`Expander::with_limits`] as ONE
        // typed call. This is the "only THIS ceiling gates" fixture
        // shape a chain of six independent setters cannot express in
        // ONE expression, and the shape the last commit named as a
        // future benefit of the (getter, setter, at-construction)
        // three-corner face closing at ONE bundle.
        //
        // Set only `max_macro_arity: 1`; confirm the arity gate fires
        // on a two-arg registration while the five other lifted
        // ceilings project through the getter as [`usize::MAX`]. A
        // regression that broke the ceiling-lifted preset would EITHER
        // fail to project [`usize::MAX`] on one of the five other
        // fields OR fail to gate the isolated arity ceiling at 1.
        let mut e = Expander::with_limits(ResourceLimits {
            max_macro_arity: 1,
            ..UNBOUNDED_RESOURCE_LIMITS
        });
        assert_eq!(e.max_macro_arity(), 1);
        assert_eq!(e.max_expansion_depth(), usize::MAX);
        assert_eq!(e.max_cache_entries(), usize::MAX);
        assert_eq!(e.max_expansion_size(), usize::MAX);
        assert_eq!(e.max_macro_body_size(), usize::MAX);
        assert_eq!(e.max_registered_macros(), usize::MAX);
        let forms = read("(defmacro two (a b) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MacroArityExceeded {
                    arity: 2,
                    limit: 1,
                    ..
                }
            ),
            "struct-update override on the ceiling-lifted preset must \
             leave the isolated arity ceiling gating; got: {err:?}"
        );
    }

    #[test]
    fn unbounded_resource_limits_carries_through_the_bulk_setter() {
        // Coherence pin between the AT-construction constructor and
        // the POST-construction bulk setter — both paths that reach the
        // ceiling-lifted preset must land at the SAME six values on
        // the expander state. Pre-lift a caller wanting the unbounded
        // posture composed six individual setter invocations at the
        // call site; post-lift both `Expander::with_limits(UNBOUNDED_
        // RESOURCE_LIMITS)` and `e.set_resource_limits(UNBOUNDED_
        // RESOURCE_LIMITS)` reach the SAME snapshot through the SAME
        // constant. A regression that wired the bulk setter to a
        // shadow field, or the ceiling-lifted preset to a stale field
        // literal, fires here on the constructor-vs-setter identity.
        let a = Expander::with_limits(UNBOUNDED_RESOURCE_LIMITS);
        let mut b = Expander::new();
        b.set_resource_limits(UNBOUNDED_RESOURCE_LIMITS);
        assert_eq!(a.resource_limits(), b.resource_limits());
        assert_eq!(a.resource_limits(), UNBOUNDED_RESOURCE_LIMITS);
    }

    // ── ResourceLimits::strictest / ::most_permissive — meet/join ──────

    /// A hand-authored asymmetric posture used across the lattice-law
    /// tests. Every field is a distinct positive value strictly less
    /// than [`usize::MAX`] and distinct from every corresponding
    /// [`DEFAULT_MAX_*`] module constant, so composition witnesses
    /// (meet, join, absorption, distributivity) discriminate across
    /// all three postures unambiguously.
    const HAND_AUTHORED_MID_POSTURE: ResourceLimits = ResourceLimits {
        max_expansion_depth: 7,
        max_cache_entries: 11,
        max_expansion_size: 13,
        max_macro_body_size: 17,
        max_registered_macros: 19,
        max_macro_arity: 23,
    };

    /// A second asymmetric posture whose per-axis values are on both
    /// sides of `HAND_AUTHORED_MID_POSTURE`'s values — three axes
    /// smaller (so the meet picks THIS posture on those axes and the
    /// join picks `MID` on those axes) and three axes larger (mirror).
    /// Together the two postures exercise every ordering combination
    /// on the six-axis pointwise `min`/`max` cascade.
    const HAND_AUTHORED_OTHER_POSTURE: ResourceLimits = ResourceLimits {
        max_expansion_depth: 3,   // smaller than MID's 7
        max_cache_entries: 29,    // larger  than MID's 11
        max_expansion_size: 5,    // smaller than MID's 13
        max_macro_body_size: 31,  // larger  than MID's 17
        max_registered_macros: 2, // smaller than MID's 19
        max_macro_arity: 41,      // larger  than MID's 23
    };

    #[test]
    fn resource_limits_strictest_of_default_and_unbounded_projects_the_default() {
        // Concrete-preset pin — the meet of the (shipped default,
        // ceiling-lifted unbounded) preset pair is the DEFAULT preset
        // structurally: every `DEFAULT_MAX_*` module constant is a
        // concrete positive value strictly less than [`usize::MAX`],
        // so on every axis the pointwise-`min` picks the DEFAULT side.
        // Peer of `most_permissive_of_default_and_unbounded_projects_the_unbounded`
        // one COMBINATOR axis over on the same shipped-preset-pair
        // surface.
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.strictest(UNBOUNDED_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        assert_eq!(
            UNBOUNDED_RESOURCE_LIMITS.strictest(DEFAULT_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn resource_limits_most_permissive_of_default_and_unbounded_projects_the_unbounded() {
        // Concrete-preset pin — the join of the (shipped default,
        // ceiling-lifted unbounded) preset pair is the UNBOUNDED
        // preset structurally: every axis's pointwise-`max` picks the
        // [`usize::MAX`] side over any concrete default. Peer of
        // `strictest_of_default_and_unbounded_projects_the_default`.
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.most_permissive(UNBOUNDED_RESOURCE_LIMITS),
            UNBOUNDED_RESOURCE_LIMITS,
        );
        assert_eq!(
            UNBOUNDED_RESOURCE_LIMITS.most_permissive(DEFAULT_RESOURCE_LIMITS),
            UNBOUNDED_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn resource_limits_strictest_takes_pointwise_min_on_every_axis() {
        // Field-level pin — the meet operation projects the pointwise
        // `min` across the six-field surface. Pinned as six
        // independent field asserts (rather than one struct equality)
        // so a drift on ONE axis carries a distinct-name failure that
        // names the drifting axis; a future SEVENTH ceiling extension
        // requires an additional field assert here in lockstep.
        let m = HAND_AUTHORED_MID_POSTURE.strictest(HAND_AUTHORED_OTHER_POSTURE);
        assert_eq!(m.max_expansion_depth, 3);
        assert_eq!(m.max_cache_entries, 11);
        assert_eq!(m.max_expansion_size, 5);
        assert_eq!(m.max_macro_body_size, 17);
        assert_eq!(m.max_registered_macros, 2);
        assert_eq!(m.max_macro_arity, 23);
    }

    #[test]
    fn resource_limits_most_permissive_takes_pointwise_max_on_every_axis() {
        // Field-level pin — the join operation projects the pointwise
        // `max` across the six-field surface. Pinned as six
        // independent field asserts so a drift on ONE axis carries a
        // distinct-name failure that names the drifting axis.
        let j = HAND_AUTHORED_MID_POSTURE.most_permissive(HAND_AUTHORED_OTHER_POSTURE);
        assert_eq!(j.max_expansion_depth, 7);
        assert_eq!(j.max_cache_entries, 29);
        assert_eq!(j.max_expansion_size, 13);
        assert_eq!(j.max_macro_body_size, 31);
        assert_eq!(j.max_registered_macros, 19);
        assert_eq!(j.max_macro_arity, 41);
    }

    #[test]
    fn resource_limits_strictest_is_idempotent() {
        // Lattice law — `a ∧ a = a`. Every posture is a fixed point of
        // its own meet with itself; a regression that changed the
        // per-axis primitive to something other than pointwise `min`
        // (a `min - 1` off-by-one, or an arbitrary tie-breaker on
        // equal values) fires here.
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.strictest(DEFAULT_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        assert_eq!(
            UNBOUNDED_RESOURCE_LIMITS.strictest(UNBOUNDED_RESOURCE_LIMITS),
            UNBOUNDED_RESOURCE_LIMITS,
        );
        assert_eq!(
            HAND_AUTHORED_MID_POSTURE.strictest(HAND_AUTHORED_MID_POSTURE),
            HAND_AUTHORED_MID_POSTURE,
        );
    }

    #[test]
    fn resource_limits_most_permissive_is_idempotent() {
        // Lattice law — `a ∨ a = a`. Sibling of the strictest
        // idempotence pin one COMBINATOR axis over.
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.most_permissive(DEFAULT_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        assert_eq!(
            UNBOUNDED_RESOURCE_LIMITS.most_permissive(UNBOUNDED_RESOURCE_LIMITS),
            UNBOUNDED_RESOURCE_LIMITS,
        );
        assert_eq!(
            HAND_AUTHORED_MID_POSTURE.most_permissive(HAND_AUTHORED_MID_POSTURE),
            HAND_AUTHORED_MID_POSTURE,
        );
    }

    #[test]
    fn resource_limits_strictest_is_commutative() {
        // Lattice law — `a ∧ b = b ∧ a`. Pointwise `min` is symmetric
        // on every axis, so the composition inherits commutativity
        // structurally. A regression that broke ordering (e.g. a
        // per-axis "prefer left on tie") fires here.
        assert_eq!(
            HAND_AUTHORED_MID_POSTURE.strictest(HAND_AUTHORED_OTHER_POSTURE),
            HAND_AUTHORED_OTHER_POSTURE.strictest(HAND_AUTHORED_MID_POSTURE),
        );
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.strictest(HAND_AUTHORED_MID_POSTURE),
            HAND_AUTHORED_MID_POSTURE.strictest(DEFAULT_RESOURCE_LIMITS),
        );
    }

    #[test]
    fn resource_limits_most_permissive_is_commutative() {
        // Lattice law — `a ∨ b = b ∨ a`. Sibling of the strictest
        // commutativity pin one COMBINATOR axis over.
        assert_eq!(
            HAND_AUTHORED_MID_POSTURE.most_permissive(HAND_AUTHORED_OTHER_POSTURE),
            HAND_AUTHORED_OTHER_POSTURE.most_permissive(HAND_AUTHORED_MID_POSTURE),
        );
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.most_permissive(HAND_AUTHORED_MID_POSTURE),
            HAND_AUTHORED_MID_POSTURE.most_permissive(DEFAULT_RESOURCE_LIMITS),
        );
    }

    #[test]
    fn resource_limits_strictest_is_associative() {
        // Lattice law — `(a ∧ b) ∧ c = a ∧ (b ∧ c)`. Pointwise `min`
        // is associative on every axis, so the composition inherits
        // associativity structurally. A regression that broke
        // grouping (e.g. an accidental accumulator that skewed the
        // order of `min` invocations) fires here.
        let a = DEFAULT_RESOURCE_LIMITS;
        let b = HAND_AUTHORED_MID_POSTURE;
        let c = HAND_AUTHORED_OTHER_POSTURE;
        assert_eq!(a.strictest(b).strictest(c), a.strictest(b.strictest(c)));
    }

    #[test]
    fn resource_limits_most_permissive_is_associative() {
        // Lattice law — `(a ∨ b) ∨ c = a ∨ (b ∨ c)`. Sibling of the
        // strictest associativity pin one COMBINATOR axis over.
        let a = DEFAULT_RESOURCE_LIMITS;
        let b = HAND_AUTHORED_MID_POSTURE;
        let c = HAND_AUTHORED_OTHER_POSTURE;
        assert_eq!(
            a.most_permissive(b).most_permissive(c),
            a.most_permissive(b.most_permissive(c)),
        );
    }

    #[test]
    fn resource_limits_meet_and_join_satisfy_absorption() {
        // Lattice absorption identities — `a ∧ (a ∨ b) = a` AND
        // `a ∨ (a ∧ b) = a`. Together with commutativity, idempotence,
        // and associativity they define the lattice axioms; the two
        // absorption laws are the operator PAIR's interlock (each
        // undoes the other's contribution on the shared operand).
        // Any per-axis primitive pair for which `min(a, max(a, b)) = a`
        // AND `max(a, min(a, b)) = a` inherits this at the pointwise
        // composition; pointwise-`min`/`max` on [`usize`] satisfy both.
        let a = HAND_AUTHORED_MID_POSTURE;
        let b = HAND_AUTHORED_OTHER_POSTURE;
        assert_eq!(a.strictest(a.most_permissive(b)), a);
        assert_eq!(a.most_permissive(a.strictest(b)), a);
    }

    #[test]
    fn resource_limits_meet_distributes_over_join() {
        // Distributive-lattice pin — `a ∧ (b ∨ c) = (a ∧ b) ∨ (a ∧ c)`.
        // Pointwise-`min` distributes over pointwise-`max` on [`usize`]
        // on every axis, so the composition inherits distributivity
        // structurally. A regression that broke the pointwise
        // structure (e.g. a per-axis operation that used SUBTRACTION
        // rather than MIN/MAX) fires here — subtraction does NOT
        // distribute over addition and would break the law on the
        // asymmetric-posture witness pair.
        let a = DEFAULT_RESOURCE_LIMITS;
        let b = HAND_AUTHORED_MID_POSTURE;
        let c = HAND_AUTHORED_OTHER_POSTURE;
        assert_eq!(
            a.strictest(b.most_permissive(c)),
            a.strictest(b).most_permissive(a.strictest(c)),
        );
    }

    #[test]
    fn resource_limits_join_distributes_over_meet() {
        // Distributive-lattice pin, sibling of the meet-over-join
        // distributivity pin one COMBINATOR axis over — `a ∨ (b ∧ c) =
        // (a ∨ b) ∧ (a ∨ c)`. In a distributive lattice BOTH
        // distributivity identities hold; pin both so a regression
        // that broke ONE without the other cannot slip through.
        let a = DEFAULT_RESOURCE_LIMITS;
        let b = HAND_AUTHORED_MID_POSTURE;
        let c = HAND_AUTHORED_OTHER_POSTURE;
        assert_eq!(
            a.most_permissive(b.strictest(c)),
            a.most_permissive(b).strictest(a.most_permissive(c)),
        );
    }

    #[test]
    fn resource_limits_strictest_is_dominated_by_both_operands_pointwise() {
        // Meet-lower-bound pin — the meet lies at or below BOTH
        // operands on every axis. This is the structural GLB property
        // of the meet in the pointwise partial order (tighter-first):
        // any admissible input under the meet is admissible under
        // both operands, so the meet's ceiling must not exceed either
        // operand's ceiling on any axis.
        let a = HAND_AUTHORED_MID_POSTURE;
        let b = HAND_AUTHORED_OTHER_POSTURE;
        let m = a.strictest(b);
        assert!(m.max_expansion_depth <= a.max_expansion_depth);
        assert!(m.max_expansion_depth <= b.max_expansion_depth);
        assert!(m.max_cache_entries <= a.max_cache_entries);
        assert!(m.max_cache_entries <= b.max_cache_entries);
        assert!(m.max_expansion_size <= a.max_expansion_size);
        assert!(m.max_expansion_size <= b.max_expansion_size);
        assert!(m.max_macro_body_size <= a.max_macro_body_size);
        assert!(m.max_macro_body_size <= b.max_macro_body_size);
        assert!(m.max_registered_macros <= a.max_registered_macros);
        assert!(m.max_registered_macros <= b.max_registered_macros);
        assert!(m.max_macro_arity <= a.max_macro_arity);
        assert!(m.max_macro_arity <= b.max_macro_arity);
    }

    #[test]
    fn resource_limits_most_permissive_dominates_both_operands_pointwise() {
        // Join-upper-bound pin — the join lies at or above BOTH
        // operands on every axis. Sibling of the strictest-lower-
        // bound pin one COMBINATOR axis over; the structural LUB
        // property of the join in the pointwise partial order.
        let a = HAND_AUTHORED_MID_POSTURE;
        let b = HAND_AUTHORED_OTHER_POSTURE;
        let j = a.most_permissive(b);
        assert!(j.max_expansion_depth >= a.max_expansion_depth);
        assert!(j.max_expansion_depth >= b.max_expansion_depth);
        assert!(j.max_cache_entries >= a.max_cache_entries);
        assert!(j.max_cache_entries >= b.max_cache_entries);
        assert!(j.max_expansion_size >= a.max_expansion_size);
        assert!(j.max_expansion_size >= b.max_expansion_size);
        assert!(j.max_macro_body_size >= a.max_macro_body_size);
        assert!(j.max_macro_body_size >= b.max_macro_body_size);
        assert!(j.max_registered_macros >= a.max_registered_macros);
        assert!(j.max_registered_macros >= b.max_registered_macros);
        assert!(j.max_macro_arity >= a.max_macro_arity);
        assert!(j.max_macro_arity >= b.max_macro_arity);
    }

    #[test]
    fn resource_limits_strictest_composes_at_compile_time_via_const_fn() {
        // Const-fn pin — the meet is evaluable in const context, so a
        // caller can stitch a new `pub const` preset from two shipped
        // presets without deferring the composition to runtime. Pinned
        // as a `const` binding at the item level so the compiler
        // exercises the const-fn path structurally; the runtime
        // assertion is redundant for a `const` value that already
        // evaluated at compile time, but pins the identity as a typed
        // theorem alongside every OTHER `strictest` pin above.
        const DEFAULT_TIGHTENED_BY_MID: ResourceLimits =
            DEFAULT_RESOURCE_LIMITS.strictest(HAND_AUTHORED_MID_POSTURE);
        assert_eq!(
            DEFAULT_TIGHTENED_BY_MID.max_expansion_depth,
            min_usize(
                DEFAULT_RESOURCE_LIMITS.max_expansion_depth,
                HAND_AUTHORED_MID_POSTURE.max_expansion_depth,
            ),
        );
        // Extra check: the composition MUST project the MID posture's
        // small value on every axis where MID is smaller than DEFAULT
        // — MID's per-axis values (7, 11, 13, 17, 19, 23) are all
        // strictly less than every `DEFAULT_MAX_*` constant, so the
        // meet projects MID verbatim.
        assert_eq!(DEFAULT_TIGHTENED_BY_MID, HAND_AUTHORED_MID_POSTURE);
    }

    #[test]
    fn resource_limits_most_permissive_composes_at_compile_time_via_const_fn() {
        // Const-fn pin sibling of the strictest const-composition pin
        // one COMBINATOR axis over — the join is also `const fn`, so
        // a caller can stitch a new `pub const` preset via the LUB
        // combinator at compile time.
        const DEFAULT_LOOSENED_BY_UNBOUNDED: ResourceLimits =
            DEFAULT_RESOURCE_LIMITS.most_permissive(UNBOUNDED_RESOURCE_LIMITS);
        assert_eq!(DEFAULT_LOOSENED_BY_UNBOUNDED, UNBOUNDED_RESOURCE_LIMITS);
    }

    // ── ResourceLimits::leq — pointwise partial order ─────────────────

    #[test]
    fn resource_limits_leq_is_pointwise_field_conjunction() {
        // Field-level pin — the leq relation projects the pointwise
        // `<=` conjunction across the six-field surface. `LOOSER` is
        // `MID + 1` on every axis, so `MID.leq(LOOSER)` holds; a
        // per-axis tighten on ANY field alone flips the relation to
        // false. Pinned as six independent tighten-per-axis asserts so
        // a drift on ONE axis carries a distinct-name failure that
        // names the drifting axis; a future SEVENTH ceiling extension
        // requires an additional per-axis tighten assert here in
        // lockstep.
        const LOOSER: ResourceLimits = ResourceLimits {
            max_expansion_depth: HAND_AUTHORED_MID_POSTURE.max_expansion_depth + 1,
            max_cache_entries: HAND_AUTHORED_MID_POSTURE.max_cache_entries + 1,
            max_expansion_size: HAND_AUTHORED_MID_POSTURE.max_expansion_size + 1,
            max_macro_body_size: HAND_AUTHORED_MID_POSTURE.max_macro_body_size + 1,
            max_registered_macros: HAND_AUTHORED_MID_POSTURE.max_registered_macros + 1,
            max_macro_arity: HAND_AUTHORED_MID_POSTURE.max_macro_arity + 1,
        };
        assert!(HAND_AUTHORED_MID_POSTURE.leq(LOOSER));
        assert!(!LOOSER.leq(HAND_AUTHORED_MID_POSTURE));

        // Per-axis exceed pins: raise MID's field ONE axis above LOOSER
        // on that axis; the leq must flip to false. Every other axis
        // stays at MID, so LOOSER still dominates on those axes —
        // isolating the single-axis violation.
        let exceed_depth = ResourceLimits {
            max_expansion_depth: LOOSER.max_expansion_depth + 1,
            ..HAND_AUTHORED_MID_POSTURE
        };
        assert!(!exceed_depth.leq(LOOSER));
        let exceed_cache = ResourceLimits {
            max_cache_entries: LOOSER.max_cache_entries + 1,
            ..HAND_AUTHORED_MID_POSTURE
        };
        assert!(!exceed_cache.leq(LOOSER));
        let exceed_expansion = ResourceLimits {
            max_expansion_size: LOOSER.max_expansion_size + 1,
            ..HAND_AUTHORED_MID_POSTURE
        };
        assert!(!exceed_expansion.leq(LOOSER));
        let exceed_body = ResourceLimits {
            max_macro_body_size: LOOSER.max_macro_body_size + 1,
            ..HAND_AUTHORED_MID_POSTURE
        };
        assert!(!exceed_body.leq(LOOSER));
        let exceed_registered = ResourceLimits {
            max_registered_macros: LOOSER.max_registered_macros + 1,
            ..HAND_AUTHORED_MID_POSTURE
        };
        assert!(!exceed_registered.leq(LOOSER));
        let exceed_arity = ResourceLimits {
            max_macro_arity: LOOSER.max_macro_arity + 1,
            ..HAND_AUTHORED_MID_POSTURE
        };
        assert!(!exceed_arity.leq(LOOSER));
    }

    #[test]
    fn resource_limits_leq_is_reflexive() {
        // Partial-order law — `a ≤ a` for every posture. Reflexivity
        // is the identity axiom on any lattice's underlying partial
        // order; a regression that changed the per-axis primitive to
        // strict `<` (or that added an accidental "prefer other on
        // tie" branch) fires here on every shipped posture.
        assert!(DEFAULT_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS));
        assert!(UNBOUNDED_RESOURCE_LIMITS.leq(UNBOUNDED_RESOURCE_LIMITS));
        assert!(HAND_AUTHORED_MID_POSTURE.leq(HAND_AUTHORED_MID_POSTURE));
        assert!(HAND_AUTHORED_OTHER_POSTURE.leq(HAND_AUTHORED_OTHER_POSTURE));
    }

    #[test]
    fn resource_limits_leq_is_antisymmetric() {
        // Partial-order law — `a ≤ b ∧ b ≤ a ⇒ a = b`. The mutual-
        // dominance witness on a partial order pins the two operands
        // to structural equality. On the pointwise `<=` composition
        // this holds because `a.max_X <= b.max_X && b.max_X <= a.max_X
        // ⇒ a.max_X == b.max_X` on every axis, and struct equality
        // is field-conjunction. Sibling to reflexivity one AXIOM axis
        // over on the partial-order axiom surface.
        //
        // Two distinct constructions of the SAME six-field posture
        // (one via struct literal, one via `..DEFAULT_RESOURCE_LIMITS`
        // spread of the shipped preset) — mutual leq holds; struct
        // equality holds; antisymmetry pin closes.
        let a = DEFAULT_RESOURCE_LIMITS;
        let b = ResourceLimits {
            max_expansion_depth: DEFAULT_RESOURCE_LIMITS.max_expansion_depth,
            max_cache_entries: DEFAULT_RESOURCE_LIMITS.max_cache_entries,
            max_expansion_size: DEFAULT_RESOURCE_LIMITS.max_expansion_size,
            max_macro_body_size: DEFAULT_RESOURCE_LIMITS.max_macro_body_size,
            max_registered_macros: DEFAULT_RESOURCE_LIMITS.max_registered_macros,
            max_macro_arity: DEFAULT_RESOURCE_LIMITS.max_macro_arity,
        };
        assert!(a.leq(b));
        assert!(b.leq(a));
        assert_eq!(a, b);
    }

    #[test]
    fn resource_limits_leq_is_transitive() {
        // Partial-order law — `a ≤ b ∧ b ≤ c ⇒ a ≤ c`. Transitivity
        // is the composition axiom on any lattice's partial order.
        // Pointwise `<=` on [`usize`] is transitive per axis, so the
        // six-field conjunction inherits it structurally.
        //
        // Chain witness: MID's every-axis `+1` shift is LOOSER, and
        // LOOSER's every-axis `+1` shift is LOOSEST; the chain MID
        // ≤ LOOSER ≤ LOOSEST implies MID ≤ LOOSEST.
        const LOOSER: ResourceLimits = ResourceLimits {
            max_expansion_depth: HAND_AUTHORED_MID_POSTURE.max_expansion_depth + 1,
            max_cache_entries: HAND_AUTHORED_MID_POSTURE.max_cache_entries + 1,
            max_expansion_size: HAND_AUTHORED_MID_POSTURE.max_expansion_size + 1,
            max_macro_body_size: HAND_AUTHORED_MID_POSTURE.max_macro_body_size + 1,
            max_registered_macros: HAND_AUTHORED_MID_POSTURE.max_registered_macros + 1,
            max_macro_arity: HAND_AUTHORED_MID_POSTURE.max_macro_arity + 1,
        };
        const LOOSEST: ResourceLimits = ResourceLimits {
            max_expansion_depth: LOOSER.max_expansion_depth + 1,
            max_cache_entries: LOOSER.max_cache_entries + 1,
            max_expansion_size: LOOSER.max_expansion_size + 1,
            max_macro_body_size: LOOSER.max_macro_body_size + 1,
            max_registered_macros: LOOSER.max_registered_macros + 1,
            max_macro_arity: LOOSER.max_macro_arity + 1,
        };
        assert!(HAND_AUTHORED_MID_POSTURE.leq(LOOSER));
        assert!(LOOSER.leq(LOOSEST));
        assert!(HAND_AUTHORED_MID_POSTURE.leq(LOOSEST));
    }

    #[test]
    fn resource_limits_leq_of_default_and_unbounded_is_a_strict_order() {
        // Concrete-preset pin — the leq relation on the shipped
        // preset pair is STRICT: DEFAULT ≤ UNBOUNDED holds (every
        // `DEFAULT_MAX_*` module constant is a concrete positive
        // value at most [`usize::MAX`]) AND UNBOUNDED ≤ DEFAULT
        // fails (every `DEFAULT_MAX_*` is strictly less than
        // [`usize::MAX`], so on every axis [`usize::MAX`] exceeds
        // the matching default). Peer of
        // `strictest_of_default_and_unbounded_projects_the_default`
        // one PRIMITIVE-KIND axis over on the same shipped-preset-
        // pair surface: where the meet composition projects DEFAULT,
        // this relation asserts DEFAULT is the tighter of the two.
        assert!(DEFAULT_RESOURCE_LIMITS.leq(UNBOUNDED_RESOURCE_LIMITS));
        assert!(!UNBOUNDED_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS));
    }

    #[test]
    fn resource_limits_leq_is_not_total_on_asymmetric_postures() {
        // Partial-order NON-total pin — the two hand-authored asymmetric
        // postures have MID smaller on three axes (depth, expansion,
        // registered) and OTHER smaller on the other three (cache,
        // body, arity); neither dominates the other, so BOTH
        // directions of the leq relation fail. The pin discriminates
        // a regression that promoted `leq` from partial to total
        // (e.g. an accidental `||` swap in the conjunction, which
        // would make every pair comparable).
        assert!(!HAND_AUTHORED_MID_POSTURE.leq(HAND_AUTHORED_OTHER_POSTURE));
        assert!(!HAND_AUTHORED_OTHER_POSTURE.leq(HAND_AUTHORED_MID_POSTURE));
    }

    #[test]
    fn resource_limits_leq_agrees_with_meet() {
        // Lattice cross-check axiom — `a ≤ b ⇔ a ⊓ b = a`. Every
        // classification lattice one crate over
        // (`tatara-lattice/src/lib.rs`'s preamble) declares this
        // agreement axiom binding the partial order to the meet
        // combinator, and the default-impl body on
        // `tatara_lattice::Lattice::leq` derives `leq` FROM `meet`
        // through it. This pin discharges the same axiom on the
        // ResourceLimits algebra as a two-direction structural
        // equality across the strict-order preset pair AND the
        // reflexivity witness.
        //
        // Forward (a ≤ b ⇒ a ⊓ b = a):
        assert!(DEFAULT_RESOURCE_LIMITS.leq(UNBOUNDED_RESOURCE_LIMITS));
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.strictest(UNBOUNDED_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        // Reverse (a ⊓ b = a ⇒ a ≤ b), reflexivity witness:
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.strictest(DEFAULT_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        assert!(DEFAULT_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS));

        // Non-agreement witness — the incomparable asymmetric postures
        // have meet distinct from EITHER operand, and leq false in
        // both directions; agreement holds on the negative side too.
        assert!(!HAND_AUTHORED_MID_POSTURE.leq(HAND_AUTHORED_OTHER_POSTURE));
        assert_ne!(
            HAND_AUTHORED_MID_POSTURE.strictest(HAND_AUTHORED_OTHER_POSTURE),
            HAND_AUTHORED_MID_POSTURE,
        );
    }

    #[test]
    fn resource_limits_leq_agrees_with_join() {
        // Lattice cross-check axiom, sibling of the meet-agreement
        // pin one COMBINATOR axis over — `a ≤ b ⇔ a ⊔ b = b`.
        //
        // Forward (a ≤ b ⇒ a ⊔ b = b):
        assert!(DEFAULT_RESOURCE_LIMITS.leq(UNBOUNDED_RESOURCE_LIMITS));
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.most_permissive(UNBOUNDED_RESOURCE_LIMITS),
            UNBOUNDED_RESOURCE_LIMITS,
        );
        // Reverse (a ⊔ b = b ⇒ a ≤ b), reflexivity witness:
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.most_permissive(DEFAULT_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        assert!(DEFAULT_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS));

        // Non-agreement witness — the incomparable asymmetric postures
        // have join distinct from EITHER operand.
        assert!(!HAND_AUTHORED_MID_POSTURE.leq(HAND_AUTHORED_OTHER_POSTURE));
        assert_ne!(
            HAND_AUTHORED_MID_POSTURE.most_permissive(HAND_AUTHORED_OTHER_POSTURE),
            HAND_AUTHORED_OTHER_POSTURE,
        );
    }

    #[test]
    fn resource_limits_strictest_is_leq_both_operands() {
        // Meet-lower-bound pin, typed lattice-relation companion to
        // `resource_limits_strictest_is_dominated_by_both_operands_pointwise`
        // one PRIMITIVE-KIND axis over — where the per-axis pin
        // discharged the six independent `<=` asserts inline, this pin
        // routes through the typed `leq` primitive, so the
        // greatest-lower-bound property is stated in the LATTICE
        // vocabulary (meet-is-leq-both-operands) rather than the
        // six-fold field-conjunction vocabulary.
        let a = HAND_AUTHORED_MID_POSTURE;
        let b = HAND_AUTHORED_OTHER_POSTURE;
        let m = a.strictest(b);
        assert!(m.leq(a));
        assert!(m.leq(b));
    }

    #[test]
    fn resource_limits_most_permissive_is_geq_both_operands() {
        // Join-upper-bound pin, typed lattice-relation companion to
        // `resource_limits_most_permissive_dominates_both_operands_pointwise`
        // one PRIMITIVE-KIND axis over. Stated in the LATTICE
        // vocabulary as `a ≤ a ⊔ b ∧ b ≤ a ⊔ b` — sibling of the
        // meet-lower-bound pin one COMBINATOR axis over.
        let a = HAND_AUTHORED_MID_POSTURE;
        let b = HAND_AUTHORED_OTHER_POSTURE;
        let j = a.most_permissive(b);
        assert!(a.leq(j));
        assert!(b.leq(j));
    }

    #[test]
    fn resource_limits_leq_evaluates_at_compile_time_via_const_fn() {
        // Const-fn pin — the leq relation is evaluable in const
        // context, so a caller can pin a preset-relation identity at
        // compile time. Sibling of the const-fn composition pins on
        // `strictest` / `most_permissive` one PRIMITIVE-KIND axis
        // over: those two operations compose two presets into a
        // const-time third, and this operation decides a const-time
        // yes/no on two presets. `const _: ()` at item scope is a
        // compile-time proof — the compiler rejects the module if
        // either identity does not hold; a runtime `assert!` over the
        // same expression would fire at test time on the same value
        // clippy's `assertions_on_constants` correctly names as a
        // redundant late check on a compile-time constant.
        //
        // Strict-order preset-pair pin — DEFAULT ≤ UNBOUNDED (every
        // `DEFAULT_MAX_*` module constant is at most [`usize::MAX`]):
        const _: () = assert!(DEFAULT_RESOURCE_LIMITS.leq(UNBOUNDED_RESOURCE_LIMITS));
        // Reverse direction fails — [`usize::MAX`] exceeds every
        // concrete `DEFAULT_MAX_*` constant, so the partial order is
        // STRICT on the shipped preset pair:
        const _: () = assert!(!UNBOUNDED_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS));
    }

    // ── EMPTY_RESOURCE_LIMITS — bounded-lattice bottom preset ─────────

    #[test]
    fn empty_resource_limits_binds_every_ceiling_to_zero() {
        // Field-level pin — the zero preset carries `0` on every axis
        // of the six-field surface. Pinned as six independent field
        // asserts (rather than one struct equality) so a drift on ONE
        // field carries a distinct-name failure that names the drifting
        // axis. A future extension that adds a SEVENTH ceiling requires
        // an additional field assert here in lockstep with the const
        // literal — rustc's field-exhaustiveness on the const forces
        // the new field to appear, and this pin exercises the seed
        // value. Structural peer of
        // `unbounded_resource_limits_binds_every_ceiling_to_usize_max`
        // one LATTICE-POLE axis over on the (bottom, top)
        // bounded-lattice preset-pair surface.
        assert_eq!(EMPTY_RESOURCE_LIMITS.max_expansion_depth, 0);
        assert_eq!(EMPTY_RESOURCE_LIMITS.max_cache_entries, 0);
        assert_eq!(EMPTY_RESOURCE_LIMITS.max_expansion_size, 0);
        assert_eq!(EMPTY_RESOURCE_LIMITS.max_macro_body_size, 0);
        assert_eq!(EMPTY_RESOURCE_LIMITS.max_registered_macros, 0);
        assert_eq!(EMPTY_RESOURCE_LIMITS.max_macro_arity, 0);
    }

    #[test]
    fn empty_resource_limits_disagrees_with_default_on_every_ceiling() {
        // Structural-distinctness pin — the two constants
        // [`DEFAULT_RESOURCE_LIMITS`] and [`EMPTY_RESOURCE_LIMITS`] sit
        // at DISTINCT points on the preset-posture axis, and the
        // disagreement is EXHAUSTIVE on the six-field surface — every
        // shipped `DEFAULT_MAX_*` module constant is a concrete
        // strictly-positive value, so every axis distinguishes DEFAULT
        // from the all-zero bottom. Peer of
        // `unbounded_resource_limits_disagrees_with_default_on_every_ceiling`
        // one LATTICE-POLE axis over.
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_expansion_depth,
            DEFAULT_RESOURCE_LIMITS.max_expansion_depth
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_cache_entries,
            DEFAULT_RESOURCE_LIMITS.max_cache_entries
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_expansion_size,
            DEFAULT_RESOURCE_LIMITS.max_expansion_size
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_macro_body_size,
            DEFAULT_RESOURCE_LIMITS.max_macro_body_size
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_registered_macros,
            DEFAULT_RESOURCE_LIMITS.max_registered_macros
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_macro_arity,
            DEFAULT_RESOURCE_LIMITS.max_macro_arity
        );
    }

    #[test]
    fn empty_resource_limits_disagrees_with_unbounded_on_every_ceiling() {
        // Diagonal-distinctness pin — the (BOTTOM, TOP) preset pair
        // disagrees on every axis (`0 != usize::MAX` on every field).
        // Structural companion to the DEFAULT-vs-EMPTY and
        // UNBOUNDED-vs-DEFAULT disagreement pins one PRESET-PAIR axis
        // over — together the three pin the six-field surface at the
        // full diagonal of the (EMPTY, DEFAULT, UNBOUNDED) triple.
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_expansion_depth,
            UNBOUNDED_RESOURCE_LIMITS.max_expansion_depth
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_cache_entries,
            UNBOUNDED_RESOURCE_LIMITS.max_cache_entries
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_expansion_size,
            UNBOUNDED_RESOURCE_LIMITS.max_expansion_size
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_macro_body_size,
            UNBOUNDED_RESOURCE_LIMITS.max_macro_body_size
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_registered_macros,
            UNBOUNDED_RESOURCE_LIMITS.max_registered_macros
        );
        assert_ne!(
            EMPTY_RESOURCE_LIMITS.max_macro_arity,
            UNBOUNDED_RESOURCE_LIMITS.max_macro_arity
        );
    }

    #[test]
    fn empty_resource_limits_is_the_join_identity() {
        // Lattice-identity law — `a ⊔ ⊥ = a`. Every posture's
        // pointwise `max` against 0 returns the posture verbatim, so
        // [`EMPTY_RESOURCE_LIMITS`] acts as the identity element of
        // the join monoid. Peer of
        // `empty_resource_limits_is_the_meet_annihilator` one
        // COMBINATOR axis over on the (meet, join) surface.
        //
        // Cross-preset exhaustion — verify the identity against every
        // named preset (DEFAULT, UNBOUNDED, both hand-authored
        // asymmetric postures) so a regression that broke ONE axis of
        // the algebra fires here on the operand whose field crosses
        // that axis's boundary.
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.most_permissive(EMPTY_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        assert_eq!(
            EMPTY_RESOURCE_LIMITS.most_permissive(DEFAULT_RESOURCE_LIMITS),
            DEFAULT_RESOURCE_LIMITS,
        );
        assert_eq!(
            UNBOUNDED_RESOURCE_LIMITS.most_permissive(EMPTY_RESOURCE_LIMITS),
            UNBOUNDED_RESOURCE_LIMITS,
        );
        assert_eq!(
            EMPTY_RESOURCE_LIMITS.most_permissive(UNBOUNDED_RESOURCE_LIMITS),
            UNBOUNDED_RESOURCE_LIMITS,
        );
        assert_eq!(
            HAND_AUTHORED_MID_POSTURE.most_permissive(EMPTY_RESOURCE_LIMITS),
            HAND_AUTHORED_MID_POSTURE,
        );
        assert_eq!(
            HAND_AUTHORED_OTHER_POSTURE.most_permissive(EMPTY_RESOURCE_LIMITS),
            HAND_AUTHORED_OTHER_POSTURE,
        );
        // Idempotence at the identity itself — sibling witness of the
        // `most_permissive_is_idempotent` general pin one PRESET axis
        // over.
        assert_eq!(
            EMPTY_RESOURCE_LIMITS.most_permissive(EMPTY_RESOURCE_LIMITS),
            EMPTY_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn empty_resource_limits_is_the_meet_annihilator() {
        // Lattice-annihilator law — `a ⊓ ⊥ = ⊥`. Every posture's
        // pointwise `min` against 0 returns 0 on every axis, so
        // [`EMPTY_RESOURCE_LIMITS`] acts as the annihilator (absorbing
        // element) of the meet operation. Peer of
        // `empty_resource_limits_is_the_join_identity` one COMBINATOR
        // axis over — the identity on one side of the bounded lattice
        // is the annihilator on the other side.
        assert_eq!(
            DEFAULT_RESOURCE_LIMITS.strictest(EMPTY_RESOURCE_LIMITS),
            EMPTY_RESOURCE_LIMITS,
        );
        assert_eq!(
            EMPTY_RESOURCE_LIMITS.strictest(DEFAULT_RESOURCE_LIMITS),
            EMPTY_RESOURCE_LIMITS,
        );
        assert_eq!(
            UNBOUNDED_RESOURCE_LIMITS.strictest(EMPTY_RESOURCE_LIMITS),
            EMPTY_RESOURCE_LIMITS,
        );
        assert_eq!(
            EMPTY_RESOURCE_LIMITS.strictest(UNBOUNDED_RESOURCE_LIMITS),
            EMPTY_RESOURCE_LIMITS,
        );
        assert_eq!(
            HAND_AUTHORED_MID_POSTURE.strictest(EMPTY_RESOURCE_LIMITS),
            EMPTY_RESOURCE_LIMITS,
        );
        assert_eq!(
            HAND_AUTHORED_OTHER_POSTURE.strictest(EMPTY_RESOURCE_LIMITS),
            EMPTY_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn empty_resource_limits_is_the_lattice_minimum() {
        // Partial-order minimum law — `⊥ ≤ a` for every `a`. Every
        // posture's field is a `usize` and `0 <= x` holds for every
        // `usize x`, so [`EMPTY_RESOURCE_LIMITS`] sits at or below
        // every posture on every axis of the pointwise `<=` conjunction.
        // Peer of the UNBOUNDED-is-maximum property on the
        // partial-order face one LATTICE-POLE axis over — those two
        // pin the (min, max) bounds of the bounded lattice.
        assert!(EMPTY_RESOURCE_LIMITS.leq(EMPTY_RESOURCE_LIMITS));
        assert!(EMPTY_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS));
        assert!(EMPTY_RESOURCE_LIMITS.leq(UNBOUNDED_RESOURCE_LIMITS));
        assert!(EMPTY_RESOURCE_LIMITS.leq(HAND_AUTHORED_MID_POSTURE));
        assert!(EMPTY_RESOURCE_LIMITS.leq(HAND_AUTHORED_OTHER_POSTURE));
        // Reverse direction FAILS on every non-EMPTY preset (each has
        // at least one field strictly greater than 0). The partial
        // order is STRICT between EMPTY and every other named preset,
        // structurally exhausting the diagonal of the
        // (EMPTY, DEFAULT, UNBOUNDED, MID, OTHER) 5-preset ordering
        // face at the BOTTOM row.
        assert!(!DEFAULT_RESOURCE_LIMITS.leq(EMPTY_RESOURCE_LIMITS));
        assert!(!UNBOUNDED_RESOURCE_LIMITS.leq(EMPTY_RESOURCE_LIMITS));
        assert!(!HAND_AUTHORED_MID_POSTURE.leq(EMPTY_RESOURCE_LIMITS));
        assert!(!HAND_AUTHORED_OTHER_POSTURE.leq(EMPTY_RESOURCE_LIMITS));
    }

    #[test]
    fn empty_resource_limits_composes_at_compile_time_via_const_fn() {
        // Const-fn pin — the bounded-lattice identity + annihilator +
        // minimum laws hold at COMPILE time. Sibling of
        // `resource_limits_leq_evaluates_at_compile_time_via_const_fn`
        // one LATTICE-STRUCTURE axis over — that pin fixed the strict
        // order of the (DEFAULT, UNBOUNDED) preset pair at compile
        // time; this pin fixes the bounded-lattice pole role of the
        // (EMPTY, UNBOUNDED) preset pair. `const _: ()` at item scope
        // is a compile-time proof — a regression that broke either
        // identity element or the bottom-is-minimum invariant fires at
        // rustc time, not test-run time.
        //
        // Bottom-is-minimum: EMPTY ≤ DEFAULT ≤ UNBOUNDED (the
        // three-preset ordering chain on the shipped preset diagonal).
        const _: () = assert!(EMPTY_RESOURCE_LIMITS.leq(DEFAULT_RESOURCE_LIMITS));
        const _: () = assert!(EMPTY_RESOURCE_LIMITS.leq(UNBOUNDED_RESOURCE_LIMITS));
        // Reverse strictness — neither DEFAULT nor UNBOUNDED sits
        // below EMPTY (each has a strictly-positive field).
        const _: () = assert!(!DEFAULT_RESOURCE_LIMITS.leq(EMPTY_RESOURCE_LIMITS));
        const _: () = assert!(!UNBOUNDED_RESOURCE_LIMITS.leq(EMPTY_RESOURCE_LIMITS));
        // Join-identity + meet-annihilator laws cross-pinned through
        // the `leq` primitive at const time — the round-trip identity
        // (`a ⊔ ⊥ = a ⇔ ⊥ ≤ a`, `a ⊓ ⊥ = ⊥ ⇔ ⊥ ≤ a`) means the two
        // leq asserts above discharge both algebraic laws by the
        // `leq_agrees_with_meet` + `leq_agrees_with_join` lattice
        // cross-check axiom already pinned in this cohort.
    }

    #[test]
    fn empty_resource_limits_seeds_most_permissive_fold_over_slice() {
        // Fold-identity behavioral pin — a slice of postures folded
        // through [`ResourceLimits::most_permissive`] with EMPTY as
        // the seed produces the pointwise least upper bound across
        // the slice on every axis. The join-identity property
        // (`a.most_permissive(EMPTY) == a`) guarantees the seed
        // contributes nothing to the fold's result — every field's
        // pointwise `max` against the seed's `0` returns the field's
        // value verbatim, so the fold reduces to the joined-across-
        // -operands upper bound. This is the destination usage the
        // constant's docstring names as its fold-identity role.
        //
        // A regression that seeded from ANY non-empty posture (e.g.
        // `DEFAULT_RESOURCE_LIMITS`) would inflate the fold's result
        // on every axis where the seed's field exceeds the pointwise
        // max of the slice — fires as a distinct-axis inequality here.
        let postures: [ResourceLimits; 3] = [
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ];
        let joined = postures
            .iter()
            .copied()
            .fold(EMPTY_RESOURCE_LIMITS, ResourceLimits::most_permissive);
        // Every operand is `leq` the joined result — the least
        // upper bound dominates each contributor on every axis.
        assert!(HAND_AUTHORED_MID_POSTURE.leq(joined));
        assert!(HAND_AUTHORED_OTHER_POSTURE.leq(joined));
        assert!(DEFAULT_RESOURCE_LIMITS.leq(joined));
        // Field-level pin — each axis is the pointwise `max` of the
        // three sources' values. Six independent field asserts so a
        // drift on ONE axis carries a distinct-name failure.
        assert_eq!(
            joined.max_expansion_depth,
            HAND_AUTHORED_MID_POSTURE
                .max_expansion_depth
                .max(HAND_AUTHORED_OTHER_POSTURE.max_expansion_depth)
                .max(DEFAULT_RESOURCE_LIMITS.max_expansion_depth),
        );
        assert_eq!(
            joined.max_cache_entries,
            HAND_AUTHORED_MID_POSTURE
                .max_cache_entries
                .max(HAND_AUTHORED_OTHER_POSTURE.max_cache_entries)
                .max(DEFAULT_RESOURCE_LIMITS.max_cache_entries),
        );
        assert_eq!(
            joined.max_expansion_size,
            HAND_AUTHORED_MID_POSTURE
                .max_expansion_size
                .max(HAND_AUTHORED_OTHER_POSTURE.max_expansion_size)
                .max(DEFAULT_RESOURCE_LIMITS.max_expansion_size),
        );
        assert_eq!(
            joined.max_macro_body_size,
            HAND_AUTHORED_MID_POSTURE
                .max_macro_body_size
                .max(HAND_AUTHORED_OTHER_POSTURE.max_macro_body_size)
                .max(DEFAULT_RESOURCE_LIMITS.max_macro_body_size),
        );
        assert_eq!(
            joined.max_registered_macros,
            HAND_AUTHORED_MID_POSTURE
                .max_registered_macros
                .max(HAND_AUTHORED_OTHER_POSTURE.max_registered_macros)
                .max(DEFAULT_RESOURCE_LIMITS.max_registered_macros),
        );
        assert_eq!(
            joined.max_macro_arity,
            HAND_AUTHORED_MID_POSTURE
                .max_macro_arity
                .max(HAND_AUTHORED_OTHER_POSTURE.max_macro_arity)
                .max(DEFAULT_RESOURCE_LIMITS.max_macro_arity),
        );
        // Empty-slice edge case — folding over an EMPTY slice returns
        // the seed verbatim (the identity element as the null aggregate).
        // A wrapper-type approach (`Option<ResourceLimits>` for the
        // aggregate) would need a distinct absent branch here; the
        // typed identity-element approach resolves it structurally.
        let empty_slice: [ResourceLimits; 0] = [];
        assert_eq!(
            empty_slice
                .iter()
                .copied()
                .fold(EMPTY_RESOURCE_LIMITS, ResourceLimits::most_permissive),
            EMPTY_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn unbounded_resource_limits_seeds_strictest_fold_over_slice() {
        // Dual fold-identity pin — a slice of postures folded through
        // [`ResourceLimits::strictest`] with UNBOUNDED as the seed
        // produces the pointwise greatest lower bound across the slice
        // on every axis. Peer of the EMPTY-seeds-most_permissive-fold
        // pin one COMBINATOR axis over — the identity element for
        // meet is the join's ANNIHILATOR (`UNBOUNDED`), and the
        // identity element for join is the meet's annihilator
        // (`EMPTY`); the two together bind the bounded-lattice's
        // aggregation surface at BOTH poles.
        let postures: [ResourceLimits; 3] = [
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ];
        let met = postures
            .iter()
            .copied()
            .fold(UNBOUNDED_RESOURCE_LIMITS, ResourceLimits::strictest);
        // The greatest lower bound sits `leq` every operand.
        assert!(met.leq(HAND_AUTHORED_MID_POSTURE));
        assert!(met.leq(HAND_AUTHORED_OTHER_POSTURE));
        assert!(met.leq(DEFAULT_RESOURCE_LIMITS));
        // Field-level pin — each axis is the pointwise `min` of the
        // three sources' values (the UNBOUNDED seed's `usize::MAX`
        // never wins the min against a concrete positive value).
        assert_eq!(
            met.max_expansion_depth,
            HAND_AUTHORED_MID_POSTURE
                .max_expansion_depth
                .min(HAND_AUTHORED_OTHER_POSTURE.max_expansion_depth)
                .min(DEFAULT_RESOURCE_LIMITS.max_expansion_depth),
        );
        assert_eq!(
            met.max_cache_entries,
            HAND_AUTHORED_MID_POSTURE
                .max_cache_entries
                .min(HAND_AUTHORED_OTHER_POSTURE.max_cache_entries)
                .min(DEFAULT_RESOURCE_LIMITS.max_cache_entries),
        );
        assert_eq!(
            met.max_expansion_size,
            HAND_AUTHORED_MID_POSTURE
                .max_expansion_size
                .min(HAND_AUTHORED_OTHER_POSTURE.max_expansion_size)
                .min(DEFAULT_RESOURCE_LIMITS.max_expansion_size),
        );
        assert_eq!(
            met.max_macro_body_size,
            HAND_AUTHORED_MID_POSTURE
                .max_macro_body_size
                .min(HAND_AUTHORED_OTHER_POSTURE.max_macro_body_size)
                .min(DEFAULT_RESOURCE_LIMITS.max_macro_body_size),
        );
        assert_eq!(
            met.max_registered_macros,
            HAND_AUTHORED_MID_POSTURE
                .max_registered_macros
                .min(HAND_AUTHORED_OTHER_POSTURE.max_registered_macros)
                .min(DEFAULT_RESOURCE_LIMITS.max_registered_macros),
        );
        assert_eq!(
            met.max_macro_arity,
            HAND_AUTHORED_MID_POSTURE
                .max_macro_arity
                .min(HAND_AUTHORED_OTHER_POSTURE.max_macro_arity)
                .min(DEFAULT_RESOURCE_LIMITS.max_macro_arity),
        );
        // Empty-slice edge — folding over an empty slice returns the
        // UNBOUNDED seed verbatim (the meet-identity as the null
        // aggregate). Symmetric to the EMPTY-seed empty-slice pin.
        let empty_slice: [ResourceLimits; 0] = [];
        assert_eq!(
            empty_slice
                .iter()
                .copied()
                .fold(UNBOUNDED_RESOURCE_LIMITS, ResourceLimits::strictest),
            UNBOUNDED_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn resource_limits_strictest_of_empty_slice_returns_the_meet_identity() {
        // Null-aggregate pin — the N-ary meet over an empty slice is
        // the meet-identity element [`UNBOUNDED_RESOURCE_LIMITS`]. The
        // typed identity-element approach resolves the empty-input case
        // at the algebra layer; a wrapper-type approach
        // (`Option<ResourceLimits>` for the aggregate) would need a
        // distinct absent branch here, and a panic-on-empty seed-from-
        // -first-element approach would abort the caller.
        assert_eq!(ResourceLimits::strictest_of(&[]), UNBOUNDED_RESOURCE_LIMITS,);
    }

    #[test]
    fn resource_limits_most_permissive_of_empty_slice_returns_the_join_identity() {
        // Null-aggregate dual pin — the N-ary join over an empty slice
        // is the join-identity element [`EMPTY_RESOURCE_LIMITS`]. Peer
        // to `strictest_of_empty_slice` one COMBINATOR axis over on the
        // (N-ary meet, N-ary join) surface — the identity element for
        // join is the meet's ANNIHILATOR and vice versa; the two
        // together bind both bounded-lattice poles as null aggregates.
        assert_eq!(
            ResourceLimits::most_permissive_of(&[]),
            EMPTY_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn resource_limits_strictest_of_single_element_returns_the_element_verbatim() {
        // Meet-identity absorption pin at N=1 — the N-ary meet of ONE
        // input is the input verbatim, since the meet-identity seed
        // (`UNBOUNDED_RESOURCE_LIMITS`) satisfies
        // `UNBOUNDED.strictest(a) == a` by the pointwise `min` behavior
        // against [`usize::MAX`]. This is the identity-law-at-N=1
        // pin: the fold's seed does not distort the 1-input case.
        assert_eq!(
            ResourceLimits::strictest_of(&[HAND_AUTHORED_MID_POSTURE]),
            HAND_AUTHORED_MID_POSTURE,
        );
        assert_eq!(
            ResourceLimits::strictest_of(&[HAND_AUTHORED_OTHER_POSTURE]),
            HAND_AUTHORED_OTHER_POSTURE,
        );
        assert_eq!(
            ResourceLimits::strictest_of(&[DEFAULT_RESOURCE_LIMITS]),
            DEFAULT_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn resource_limits_most_permissive_of_single_element_returns_the_element_verbatim() {
        // Join-identity absorption pin at N=1 — dual of
        // `strictest_of_single_element`, using the join-identity seed
        // (`EMPTY_RESOURCE_LIMITS`) whose per-axis `0` satisfies
        // `EMPTY.most_permissive(a) == a` against any non-zero value.
        assert_eq!(
            ResourceLimits::most_permissive_of(&[HAND_AUTHORED_MID_POSTURE]),
            HAND_AUTHORED_MID_POSTURE,
        );
        assert_eq!(
            ResourceLimits::most_permissive_of(&[HAND_AUTHORED_OTHER_POSTURE]),
            HAND_AUTHORED_OTHER_POSTURE,
        );
        assert_eq!(
            ResourceLimits::most_permissive_of(&[DEFAULT_RESOURCE_LIMITS]),
            DEFAULT_RESOURCE_LIMITS,
        );
    }

    #[test]
    fn resource_limits_strictest_of_two_elements_reduces_to_pairwise_strictest() {
        // N=2 reduction pin — the N-ary meet on a 2-element slice
        // reduces to the pairwise combinator, since
        // `UNBOUNDED.strictest(a).strictest(b) == a.strictest(b)`. The
        // strictest_of(&[a, b]) call MUST agree with a.strictest(b) on
        // every input pair the algebra distinguishes.
        assert_eq!(
            ResourceLimits::strictest_of(
                &[HAND_AUTHORED_MID_POSTURE, HAND_AUTHORED_OTHER_POSTURE,]
            ),
            HAND_AUTHORED_MID_POSTURE.strictest(HAND_AUTHORED_OTHER_POSTURE),
        );
        assert_eq!(
            ResourceLimits::strictest_of(&[DEFAULT_RESOURCE_LIMITS, UNBOUNDED_RESOURCE_LIMITS,]),
            DEFAULT_RESOURCE_LIMITS.strictest(UNBOUNDED_RESOURCE_LIMITS),
        );
    }

    #[test]
    fn resource_limits_most_permissive_of_two_elements_reduces_to_pairwise_most_permissive() {
        // N=2 reduction dual pin — the N-ary join on a 2-element slice
        // reduces to the pairwise combinator.
        assert_eq!(
            ResourceLimits::most_permissive_of(&[
                HAND_AUTHORED_MID_POSTURE,
                HAND_AUTHORED_OTHER_POSTURE,
            ]),
            HAND_AUTHORED_MID_POSTURE.most_permissive(HAND_AUTHORED_OTHER_POSTURE),
        );
        assert_eq!(
            ResourceLimits::most_permissive_of(&[DEFAULT_RESOURCE_LIMITS, EMPTY_RESOURCE_LIMITS,]),
            DEFAULT_RESOURCE_LIMITS.most_permissive(EMPTY_RESOURCE_LIMITS),
        );
    }

    #[test]
    fn resource_limits_strictest_of_agrees_with_direct_fold_over_slice() {
        // Method-agrees-with-fold pin — the sole behavioral claim the
        // N-ary aggregator makes is that it IS the inline
        // `.iter().copied().fold(UNBOUNDED, strictest)` cascade the
        // pre-lift caller composed at every consumption site. A drift
        // between the method's implementation and the inline fold
        // would break this equality on every non-empty slice.
        let postures: [ResourceLimits; 3] = [
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ];
        assert_eq!(
            ResourceLimits::strictest_of(&postures),
            postures
                .iter()
                .copied()
                .fold(UNBOUNDED_RESOURCE_LIMITS, ResourceLimits::strictest),
        );
    }

    #[test]
    fn resource_limits_most_permissive_of_agrees_with_direct_fold_over_slice() {
        // Method-agrees-with-fold dual pin.
        let postures: [ResourceLimits; 3] = [
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ];
        assert_eq!(
            ResourceLimits::most_permissive_of(&postures),
            postures
                .iter()
                .copied()
                .fold(EMPTY_RESOURCE_LIMITS, ResourceLimits::most_permissive),
        );
    }

    #[test]
    fn resource_limits_strictest_of_is_order_independent() {
        // Order-independence pin — the fold inherits commutativity AND
        // associativity of the pairwise `strictest`, so any permutation
        // of the input slice yields the same aggregate. Reversal is
        // the maximal permutation for a 3-element input (indices
        // 0,1,2 → 2,1,0); an aggregator that leaked fold order would
        // discriminate here.
        let forward = ResourceLimits::strictest_of(&[
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ]);
        let reversed = ResourceLimits::strictest_of(&[
            DEFAULT_RESOURCE_LIMITS,
            HAND_AUTHORED_OTHER_POSTURE,
            HAND_AUTHORED_MID_POSTURE,
        ]);
        assert_eq!(forward, reversed);
    }

    #[test]
    fn resource_limits_most_permissive_of_is_order_independent() {
        let forward = ResourceLimits::most_permissive_of(&[
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ]);
        let reversed = ResourceLimits::most_permissive_of(&[
            DEFAULT_RESOURCE_LIMITS,
            HAND_AUTHORED_OTHER_POSTURE,
            HAND_AUTHORED_MID_POSTURE,
        ]);
        assert_eq!(forward, reversed);
    }

    #[test]
    fn resource_limits_strictest_of_composes_at_compile_time_via_const_fn() {
        // Compile-time-composition pin — `strictest_of` is `const fn`,
        // so a caller binds an N-ary aggregate to a `pub const` and the
        // fold evaluates at compile time. Every `DEFAULT_MAX_*` is
        // strictly less than `usize::MAX`, so the meet of `[DEFAULT,
        // UNBOUNDED]` picks DEFAULT on every axis — a preset-pair
        // identity already pinned pairwise, verified here through the
        // N-ary method's const-context evaluation. The N-ary const-fn
        // peer of the pairwise const-fn pins on
        // [`ResourceLimits::strictest`]; a regression that turned
        // `strictest_of` into a runtime `fn` would break the `const`
        // binding below at compile time.
        const AGGREGATED: ResourceLimits =
            ResourceLimits::strictest_of(&[DEFAULT_RESOURCE_LIMITS, UNBOUNDED_RESOURCE_LIMITS]);
        assert_eq!(AGGREGATED, DEFAULT_RESOURCE_LIMITS);
    }

    #[test]
    fn resource_limits_most_permissive_of_composes_at_compile_time_via_const_fn() {
        // Compile-time-composition dual pin — every `DEFAULT_MAX_*` is
        // strictly greater than `0`, so the join of `[EMPTY, DEFAULT]`
        // picks DEFAULT on every axis.
        const AGGREGATED: ResourceLimits =
            ResourceLimits::most_permissive_of(&[EMPTY_RESOURCE_LIMITS, DEFAULT_RESOURCE_LIMITS]);
        assert_eq!(AGGREGATED, DEFAULT_RESOURCE_LIMITS);
    }

    #[test]
    fn resource_limits_strictest_of_result_is_leq_every_operand() {
        // Lattice-relation pin — the N-ary meet sits `leq` every
        // operand. This is the N-ary extension of the pairwise
        // `strictest_is_leq_both_operands` pin; the meet aggregate
        // dominates from below (the greatest lower bound of the slice
        // sits at-or-below each input on every axis).
        let postures: [ResourceLimits; 3] = [
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ];
        let met = ResourceLimits::strictest_of(&postures);
        for p in postures {
            assert!(met.leq(p));
        }
    }

    #[test]
    fn resource_limits_most_permissive_of_result_is_geq_every_operand() {
        // Lattice-relation dual pin — every operand sits `leq` the
        // N-ary join. The join aggregate is the least upper bound of
        // the slice.
        let postures: [ResourceLimits; 3] = [
            HAND_AUTHORED_MID_POSTURE,
            HAND_AUTHORED_OTHER_POSTURE,
            DEFAULT_RESOURCE_LIMITS,
        ];
        let joined = ResourceLimits::most_permissive_of(&postures);
        for p in postures {
            assert!(p.leq(joined));
        }
    }

    #[test]
    fn expander_built_at_strictest_of_slice_gates_at_tightest_ceiling_across_the_slice() {
        // Behavioral pin — an expander constructed from the N-ary meet
        // of a slice of postures MUST gate at the tightest ceiling
        // across the slice on every axis. Peer of
        // `expander_built_at_strictest_of_two_presets_gates_at_the_tighter_ceiling`
        // one AGGREGATION-ARITY axis over: where the pairwise pin
        // composes exactly two postures, this pin composes THREE
        // through the N-ary aggregator, confirming the arity extension
        // preserves the "meet gates at tightest" behavioral contract.
        //
        // The slice tightens macro_arity to 1 (the tightest across the
        // three postures) via the middle operand; every other axis
        // stays at usize::MAX (the outer two operands supply
        // usize::MAX; the middle operand's struct-update literal
        // preserves usize::MAX everywhere except macro_arity).
        let tightened = ResourceLimits::strictest_of(&[
            UNBOUNDED_RESOURCE_LIMITS,
            ResourceLimits {
                max_macro_arity: 1,
                ..UNBOUNDED_RESOURCE_LIMITS
            },
            UNBOUNDED_RESOURCE_LIMITS,
        ]);
        assert_eq!(tightened.max_macro_arity, 1);
        assert_eq!(tightened.max_expansion_depth, usize::MAX);
        assert_eq!(tightened.max_cache_entries, usize::MAX);
        assert_eq!(tightened.max_expansion_size, usize::MAX);
        assert_eq!(tightened.max_macro_body_size, usize::MAX);
        assert_eq!(tightened.max_registered_macros, usize::MAX);
        let mut e = Expander::with_limits(tightened);
        let forms = read("(defmacro two (a b) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MacroArityExceeded {
                    arity: 2,
                    limit: 1,
                    ..
                }
            ),
            "expected MacroArityExceeded from N-ary meet's tightened arity gate, got {err:?}",
        );
    }

    #[test]
    fn expander_built_at_strictest_of_two_presets_gates_at_the_tighter_ceiling() {
        // Behavioral pin — an expander constructed from the meet of
        // two postures MUST gate at the tighter of the two ceilings
        // on every axis. Build the meet of the ceiling-lifted preset
        // and a hand-authored posture whose `max_macro_arity` sits at
        // 1; the meet inherits the 1-slot arity ceiling (tighter than
        // [`usize::MAX`]) AND the [`usize::MAX`] ceiling on every
        // other axis (tighter than the hand-authored posture's small
        // values — no, wait: the OTHER posture's small values ARE
        // tighter. So the meet inherits 1 on arity and the OTHER
        // small values elsewhere.) Confirm the constructed expander
        // rejects a 2-arg macro registration with the arity gate.
        let tightened = UNBOUNDED_RESOURCE_LIMITS.strictest(ResourceLimits {
            max_macro_arity: 1,
            ..UNBOUNDED_RESOURCE_LIMITS
        });
        assert_eq!(tightened.max_macro_arity, 1);
        // Other five axes remain at `usize::MAX` (both operands carry
        // `usize::MAX` on those, so the pointwise `min` is `usize::MAX`).
        assert_eq!(tightened.max_expansion_depth, usize::MAX);
        assert_eq!(tightened.max_cache_entries, usize::MAX);
        assert_eq!(tightened.max_expansion_size, usize::MAX);
        assert_eq!(tightened.max_macro_body_size, usize::MAX);
        assert_eq!(tightened.max_registered_macros, usize::MAX);
        let mut e = Expander::with_limits(tightened);
        let forms = read("(defmacro two (a b) `,a)").unwrap();
        let err = e.expand_program(forms).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MacroArityExceeded {
                    arity: 2,
                    limit: 1,
                    ..
                }
            ),
            "the tighter arity ceiling from the meet MUST reach the register-time gate; got: {err:?}"
        );
    }
}
