# Home-manager module — operator-facing config for tatara-reconciler's
# ephemeral envelope. The YAML this writes is consumed by the binary
# via shikumi's load_and_watch path (XDG search +
# TATARA_RECONCILER_EPHEMERAL_* env override).
#
# Wiring: this module emits the typed config struct at
#   $XDG_CONFIG_HOME/tatara-reconciler/ephemeral.yaml
# tatara-reconciler resolves it via shikumi::ConfigStore<EphemeralDefaults>
# in its main(); hot-reload picks up edits within ~250ms.

{ config, lib, pkgs, ... }:

let
  cfg = config.programs.tatara-reconciler.ephemeral;
  yamlGenerator = pkgs.formats.yaml { };

  # Selective YAML rendering — empty fields drop out so the file stays
  # minimal and shikumi's defaults fill them in.
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
  options.programs.tatara-reconciler.ephemeral = {
    enable = lib.mkEnableOption "tatara-reconciler ephemeral defaults config";

    defaultTtl = lib.mkOption {
      type = lib.types.str;
      default = "1h";
      example = "30m";
      description = ''
        Default TTL when a Process's `lifetime.ephemeral.ttl` field is
        empty. humantime format (e.g., "1h", "30m", "2h30m").
      '';
    };

    maxConcurrentPerCluster = lib.mkOption {
      type = lib.types.ints.unsigned;
      default = 0;
      example = 8;
      description = ''
        Maximum concurrent ephemeral Processes cluster-wide. 0 = no cap.
        Enforced before transitioning out of Pending.
      '';
    };

    registry = lib.mkOption {
      type = lib.types.str;
      default = "ghcr.io/pleme-io/charts";
      example = "ghcr.io/example/charts";
      description = ''
        Default OCI registry for Aplicacao chart refs.
      '';
    };

    rootCaName = lib.mkOption {
      type = lib.types.str;
      default = "saguao-fleet-root";
      description = ''
        Name of the saguão cluster-wide root CA. PROVISIONING phase
        auto-creates per-namespace intermediate Issuers chained to this
        root when the namespace carries label
        `tatara.pleme.io/ephemeral=true`.
      '';
    };

    defaultChartRef = lib.mkOption {
      type = lib.types.str;
      default = "";
      example = "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment";
      description = ''
        Default chart reference used when `(defephemeral …)` omits
        `:chart-ref`. Empty = no fallback (operator must specify).
      '';
    };

    emitOciRepository = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Whether to auto-emit an `OCIRepository` peer for `oci://` chart
        refs. Disable when OCIRepositories are pre-created cluster-wide.
      '';
    };

    manageConfig = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Whether home-manager writes `~/.config/tatara-reconciler/
        ephemeral.yaml`. Set false to leave the file untouched (env
        vars + shikumi defaults still apply).
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    home.file."${config.xdg.configHome}/tatara-reconciler/ephemeral.yaml" =
      lib.mkIf cfg.manageConfig {
        source = yamlGenerator.generate "tatara-reconciler-ephemeral.yaml" configValue;
      };
  };
}
