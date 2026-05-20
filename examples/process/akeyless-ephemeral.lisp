;; akeyless-ephemeral.lisp — reference (defephemeral …) form.
;;
;; Closed-loop attested Akeyless SaaS + Gateway in a single ephemeral
;; environment. The bundled SaaS issues credentials that authenticate
;; the bundled Gateway against itself; the closed-loop-probe Job emits
;; a tatara-receipt/v1 BLAKE3 envelope; tatara-reconciler chains the
;; receipt into the Process attestation; teardown_policy fires SIGTERM
;; on Attested → Exiting → Zombie → Reaped.
;;
;; Booleans use Scheme syntax (#t / #f) — bare true/false are symbols
;; that deserialize as strings, which silently breaks bool overlays.

(defephemeral akeyless-closed-loop-attest
  :aplicacao
    (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
     :version "0.5.5"
     :profile "gateway-with-internal-saas"
     :values-overlay
       (:cluster (:name "ephemeral-test-01")
        :data (:mysql    (:persistence (:enabled #f))
               :rabbitmq (:persistence (:enabled #f)))
        :compliance (:overlays [])
        :closedLoopProbe
          (:enabled #t
           :issuer   (:service "akeyless-saas-akeyless-gator"   :port 8080)
           :consumer (:service "akeyless-saas-akeyless-gateway" :port 8000)))
     :release-name "akeyless-saas-consolidated"
     :target-namespace "akeyless-test"
     :install-timeout "25m")
  :ttl "1h"
  :teardown OnAttested
  :max-concurrent 1
  :postconditions
    ((:kind HelmReleaseReleased
      :params (:name "akeyless-saas-consolidated"
               :namespace "akeyless-test"))
     (:kind ClosedLoopAuth
      :params (:issuer
                 (:service "akeyless-saas-akeyless-gator"
                  :port 8080
                  :jwksPath "/.well-known/jwks.json")
               :consumer
                 (:service "akeyless-saas-akeyless-gateway"
                  :port 8000
                  :authPath "/v2/whoami")
               :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
