# The same Process, declared as a Nix module.
# Intended to be rendered to YAML via `lib.generators.toYAML` and applied
# as a standard tatara.pleme.io/v1alpha1 Process.

{ ... }:

{
  services.tatara.processes."observability-stack" = {
    namespace = "seph";
    spec = {
      identity.parent = "seph.1";
      classification = {
        pointType = "Gate";
        substrate = "Observability";
        horizon.kind = "Bounded";
        calm = "Monotone";
        dataClassification = "Internal";
      };
      intent.nix = {
        flakeRef = "github:pleme-io/k8s?dir=shared/infrastructure";
        attribute = "observability";
        atticCache = "main";
      };
      boundary = {
        preconditions = [{
          kind = "ProcessPhase";
          params = { processRef = "akeyless-injection"; phase = "Attested"; };
        }];
        postconditions = [
          { kind = "KustomizationHealthy"; params = { name = "observability-stack"; namespace = "flux-system"; }; }
          { kind = "HelmReleaseReleased";  params = { name = "kube-prometheus-stack"; namespace = "observability"; }; }
          { kind = "PromQL";               params = { query = "up{job='prometheus'} == 1"; }; }
        ];
        timeout = "15m";
      };
      compliance = {
        baseline = "fedramp-moderate";
        bindings = [
          { framework = "nist-800-53"; controlId = "SC-7"; phase = "AtBoundary"; description = "Boundary protection"; }
          { framework = "cis-k8s-v1.8"; controlId = "5.1.1"; phase = "PostConvergence"; }
        ];
      };
      dependsOn = [{
        name = "akeyless-injection";
        mustReach = "Attested";
      }];
      signals = {
        sigtermGraceSeconds = 480;
        sigkillForce = true;
        sighupStrategy = "Reconverge";
      };
    };
  };
}
