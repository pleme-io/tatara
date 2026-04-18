;; checks.lisp — workspace coherence checks, read by `tatara-check`.
;;
;; Two layers:
;;   1. PRIMITIVES — typed executors in tatara-check.rs
;;        crd-in-sync · yaml-parses · yaml-parses-as · lisp-compiles · file-contains
;;        plus sequencing via (do …)
;;
;;   2. USER CHECKS via defcheck — Lisp macros that expand into one or more
;;      primitive invocations. Demonstrates Tier 1: Lisp as config language
;;      for a typed Rust-executed domain.
;;
;; Run:  cargo run --bin tatara-check -p tatara-reconciler

;; ─── user-defined check primitives ────────────────────────────────

;; Assert that a Process named across three rendering surfaces
;; (YAML, Lisp, Nix) round-trips consistently.
(defcheck process-example-triple (yaml-path lisp-path nix-path)
  `(do (yaml-parses-as Process ,yaml-path)
       (lisp-compiles ,lisp-path
                      :min-definitions 1
                      :requires (intent-nix depends-on boundary-post compliance))
       (file-contains ,nix-path
                      :strings ("services.tatara.processes" "pointType"))))

;; Assert a Helm chart's root files parse as YAML.
(defcheck chart-roots-parse (chart-yaml values-yaml)
  `(do (yaml-parses ,chart-yaml)
       (yaml-parses ,values-yaml)))

;; ─── CRDs match Rust types ────────────────────────────────────────
(crd-in-sync Process      "chart/tatara-reconciler/crds/processes.yaml")
(crd-in-sync ProcessTable "chart/tatara-reconciler/crds/processtables.yaml")

;; ─── Helm chart surface (via user macro) ──────────────────────────
(chart-roots-parse "chart/tatara-reconciler/Chart.yaml"
                   "chart/tatara-reconciler/values.yaml")

;; ─── Process example surface (via user macro) ─────────────────────
(process-example-triple
  "examples/process/observability-stack.yaml"
  "examples/process/observability-stack.lisp"
  "examples/process/observability-stack.nix")

;; ─── Tier 1 registry demo ─────────────────────────────────────────
;; These keywords aren't built-in — they're `#[derive(TataraDomain)]`
;; types from `tatara-domains`, registered at startup by tatara-check.
;; The dispatcher falls through to the registry and compiles them via
;; the derive-generated compile_from_sexp.
(defmonitor
  :name "prometheus-up"
  :query "up{job='prometheus'} == 1"
  :threshold 0.99
  :window-seconds 300
  :tags ("prod" "observability")
  :enabled #t)

(defnotify
  :name "oncall-slack"
  :channel "slack"
  :target "#alerts-prod"
  :severity "critical")

;; Richer domain: enum + nested Vec<Struct> via the Deserialize fallthrough.
(defalertpolicy
  :name "prod-outage"
  :monitor-ref "prometheus-up"
  :severity Critical
  :mute-minutes 30.0
  :mute-on-deploy #t
  :labels ("prod" "pager")
  :escalations (
    (:notify-ref "oncall"       :wait-minutes 0 :severity Page)
    (:notify-ref "slack-alerts" :wait-minutes 5 :severity Warning)))
