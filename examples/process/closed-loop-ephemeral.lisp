;; closed-loop-ephemeral.lisp — reference (defephemeral …) form.
;;
;; Closed-loop attested issuer + gateway in a single ephemeral
;; environment. The bundled issuer mints credentials that authenticate
;; the bundled gateway against itself; the closed-loop-probe Job emits
;; a tatara-receipt/v1 BLAKE3 envelope; tatara-reconciler chains the
;; receipt into the Process attestation; teardown_policy fires SIGTERM
;; on Attested → Exiting → Zombie → Reaped.
;;
;; Booleans use Scheme syntax (#t / #f) — bare true/false are symbols
;; that deserialize as strings, which silently breaks bool overlays.

(defephemeral closed-loop-attest
  :aplicacao
    (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
     :version "0.5.5"
     :profile "all-in-one"
     :values-overlay
       (:cluster (:name "ephemeral-test-01")
        :data (:mysql    (:persistence (:enabled #f))
               :rabbitmq (:persistence (:enabled #f)))
        :compliance (:overlays [])
        :closedLoopProbe
          (:enabled #t
           :issuer   (:service "demo-app-issuer"   :port 8080)
           :consumer (:service "demo-app-gateway" :port 8000)))
     :release-name "demo-app-consolidated"
     :target-namespace "demo-test"
     :install-timeout "25m")
  :ttl "1h"
  :teardown OnAttested
  :max-concurrent 1
  :postconditions
    ((:kind HelmReleaseReleased
      :params (:name "demo-app-consolidated"
               :namespace "demo-test"))
     (:kind ClosedLoopAuth
      :params (:issuer
                 (:service "demo-app-issuer"
                  :port 8080
                  :jwksPath "/.well-known/jwks.json")
               :consumer
                 (:service "demo-app-gateway"
                  :port 8000
                  :authPath "/v2/whoami")
               :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
