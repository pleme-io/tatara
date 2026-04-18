# tatara-lisp-derive

A single proc-macro: `#[derive(TataraDomain)]`. Apply it to any Rust struct
with `serde::Deserialize` and the struct gains a full Lisp authoring surface.

## Minimum example

```rust
use serde::{Serialize, Deserialize};
use tatara_lisp::DeriveTataraDomain;       // ← the derive (re-exported from tatara-lisp)
use tatara_lisp::domain::TataraDomain;     // ← the trait the derive implements

#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug)]
#[tatara(keyword = "defmonitor")]
pub struct MonitorSpec {
    pub name: String,
    pub query: String,
    pub threshold: f64,
    pub window_seconds: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub enabled: Option<bool>,
}

let forms = tatara_lisp::read(r#"
    (defmonitor :name "prom-up"
                :query "up{job='prometheus'}"
                :threshold 0.99
                :window-seconds 300
                :tags ("prod" "observability")
                :enabled #t)
"#).unwrap();
let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
```

## What gets generated

```rust
impl ::tatara_lisp::domain::TataraDomain for MonitorSpec {
    const KEYWORD: &'static str = "defmonitor";
    fn compile_from_args(args: &[::tatara_lisp::Sexp]) -> ::tatara_lisp::Result<Self> {
        let kw = ::tatara_lisp::domain::parse_kwargs(args)?;
        Ok(Self {
            name:           ::tatara_lisp::domain::extract_string(&kw, "name")?.to_string(),
            query:          ::tatara_lisp::domain::extract_string(&kw, "query")?.to_string(),
            threshold:      ::tatara_lisp::domain::extract_float(&kw, "threshold")? as f64,
            window_seconds: ::tatara_lisp::domain::extract_optional_int(&kw, "window-seconds")?,
            tags: if kw.contains_key("tags") {
                ::tatara_lisp::domain::extract_string_list(&kw, "tags")?
            } else {
                ::std::default::Default::default()
            },
            enabled:        ::tatara_lisp::domain::extract_optional_bool(&kw, "enabled")?,
        })
    }
}
```

## Keyword derivation

- `#[tatara(keyword = "defmonitor")]` — explicit (recommended)
- Default: strip `"Spec"` suffix + prefix `"def"` + lowercase. `MonitorSpec` →
  `"defmonitor"`.

## Field name ↔ keyword mapping

Snake-case field names become kebab-case Lisp keywords:
- `name` → `:name`
- `window_seconds` → `:window-seconds`
- `delegate_to_nix_build` → `:delegate-to-nix-build`

Matches Lisp tradition; also reversible via `camel_to_kebab` used by the Sexp ↔
serde_json bridge.

## Supported field types (v0)

**First-class** (purpose-built extractors):
- `String`, `Option<String>`, `Vec<String>`
- `i64`, `i32`, `u32`, `u64`, `usize`, `Option<int>`
- `f64`, `f32`, `Option<float>`
- `bool`, `Option<bool>`

**Universal fallthrough** (via `sexp_to_json` + `serde_json::from_value`):
- Any `enum` deriving `Deserialize` (bare Lisp symbol → JSON string → enum)
- Any nested `struct` deriving `Deserialize` (Lisp kwargs → JSON object → struct)
- `Option<T>` where `T: Deserialize` (missing keyword → `None`)
- `Vec<T>` where `T: Deserialize` (Lisp list of kwargs-lists → JSON array of objects → Vec)

## `#[serde(default)]` — honored

Fields marked with `#[serde(default)]` become optional: if the Lisp form omits
the keyword, the field falls back to `Default::default()`. The derive inspects
every field's `#[serde(...)]` attribute list and wraps the extractor with a
missing-key short-circuit when it finds `default`.

## Coexists cleanly with other derives

Every pleme typed struct that derives `CustomResource` + `Serialize` +
`Deserialize` + `JsonSchema` can also derive `TataraDomain`:

```rust
#[derive(CustomResource, DeriveTataraDomain, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(group = "tatara.pleme.io", version = "v1alpha1", kind = "Process", ...)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defpoint")]
pub struct ProcessSpec { ... }
```

All four attribute namespaces (`#[kube]`, `#[serde]`, `#[tatara]`, plus `#[derive]`
itself) are disjoint.

## Production example

`tatara-process::ProcessSpec` uses this derive — 8 fields with nested structs
(Classification, Intent, Boundary, ComplianceSpec, SignalPolicy, IdentitySpec),
nested enums (ConvergencePointType, SubstrateType, Severity, VerificationPhase,
MustReachPhase, SighupStrategy), `Vec<DependsOn>` of nested struct-with-enum.
**The derive handles every field. One line of macro, zero hand-rolling.**

See the full typed roundtrip test in
`tatara-process/src/lib.rs::compile_tests::full_processspec_round_trip_via_derive`.

## Minimum deps

Proc-macro crate itself:
- `syn = "2"` (with `full` feature)
- `quote = "1"`
- `proc-macro2 = "1"`

Users of the derive need:
- `tatara-lisp` on their dep list (provides trait + extractors + `Sexp`)
- `serde_json = "1"` if they use non-basic field types (derive emits `serde_json::from_value` calls)
- `serde = { version = "1", features = ["derive"] }` so the type derives `Deserialize`

## Not yet supported

- Tuple structs
- Enums (you can use enum *fields*, but the top-level `#[derive]` requires a struct)
- Custom error messages per field
- Positional args (use `compile_named` in tatara-lisp for `(keyword NAME :k v …)` shape)

All tractable; none have been needed yet.
