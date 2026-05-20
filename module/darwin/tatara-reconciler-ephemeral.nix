# nix-darwin module — operator workstation defaults for
# tatara-reconciler's ephemeral envelope. Mirrors the HM module's surface
# but uses environment.etc (nix-darwin's analogue to NixOS) when the
# operator runs the reconciler outside HM.
#
# In practice operators on macOS use the HM module path; this surface
# exists for parity + for system-wide darwin installs that aren't HM.

{ config, lib, pkgs, ... }:

let
  cfg = config.services.tatara-reconciler.ephemeral;
  yamlGenerator = pkgs.formats.yaml { };

  pruneEmpty = attrs:
    lib.filterAttrs (n: v: !(v == null || v == "" || v == { } || v == [ ])) attrs;

  configValue = pruneEmpty {
    default_ttl = cfg.defaultTtl;
    max_concurrent_per_cluster = cfg.maxConcurrentPerCluster;
    registry = cfg.registry;
    root_ca_name = cfg.rootCaName;
    default_chart_ref = cfg.defaultChartRef;
    emit_oci_repository = cfg.emitOciRepository;
  };
in
{
  options.services.tatara-reconciler.ephemeral = {
    enable = lib.mkEnableOption "tatara-reconciler ephemeral defaults (darwin)";

    defaultTtl = lib.mkOption {
      type = lib.types.str;
      default = "1h";
      description = "Default TTL for ephemeral Processes (humantime).";
    };

    maxConcurrentPerCluster = lib.mkOption {
      type = lib.types.ints.unsigned;
      default = 0;
      description = "Max concurrent ephemeral Processes cluster-wide (0 = no cap).";
    };

    registry = lib.mkOption {
      type = lib.types.str;
      default = "ghcr.io/pleme-io/charts";
      description = "Default OCI registry for Aplicacao chart refs.";
    };

    rootCaName = lib.mkOption {
      type = lib.types.str;
      default = "saguao-fleet-root";
      description = "Cluster-wide root CA name for per-namespace Issuer derivation.";
    };

    defaultChartRef = lib.mkOption {
      type = lib.types.str;
      default = "";
      description = "Fallback Aplicacao chart ref when (defephemeral …) omits :chart-ref.";
    };

    emitOciRepository = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Auto-emit OCIRepository peer for oci:// chart refs.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.etc."tatara-reconciler/ephemeral.yaml".source =
      yamlGenerator.generate "tatara-reconciler-ephemeral.yaml" configValue;
  };
}
