;;; Same Process, declared in tatara-lisp — now via `#[derive(TataraDomain)]`
;;; on ProcessSpec itself. Every keyword = kebab-case of the Rust field name;
;;; every nested kwargs block = a nested struct. The derive + serde do all
;;; the parsing — zero hand-rolled code.
;;;
;;; Ergonomic aliases are available via user macros (see obs-macros example
;;; below) — but this file uses the canonical form to prove the derive works
;;; directly against the typed schema.

(defpoint observability-stack
  :identity       (:parent "seph.1")
  :classification (:point-type          Gate
                   :substrate           Observability
                   :horizon             (:kind Bounded)
                   :calm                Monotone
                   :data-classification Internal)
  :intent         (:nix (:flake-ref "github:pleme-io/k8s?dir=shared/infrastructure"
                         :attribute  "observability"
                         :attic-cache "main"))
  :boundary       (:postconditions
                     ((:kind   KustomizationHealthy
                       :params (:name "observability-stack"
                                :namespace "flux-system"))
                      (:kind   HelmReleaseReleased
                       :params (:name "kube-prometheus-stack"
                                :namespace "observability"))
                      (:kind   PromQL
                       :params (:query "up{job='prometheus'} == 1")))
                   :timeout "15m")
  :compliance     (:baseline "fedramp-moderate"
                   :bindings
                     ((:framework "nist-800-53"
                       :control-id "SC-7"
                       :phase AtBoundary
                       :description "Boundary protection")
                      (:framework "cis-k8s-v1.8"
                       :control-id "5.1.1"
                       :phase PostConvergence)))
  :depends-on     ((:name "akeyless-injection" :must-reach Attested))
  :signals        (:sigterm-grace-seconds 480
                   :sighup-strategy       Reconverge))
