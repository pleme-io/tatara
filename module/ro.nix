# ro (炉) — Nix build platform integration
#
# Declarative configuration for connecting to the ro cluster's
# remote Nix builder and Attic binary cache. This module:
#   - Configures SSH for the remote builder endpoint
#   - Installs attic-client for cache interaction
#   - Exports builder and cache settings as module options
#     that the nix repo can consume for nix-darwin config
#
# System-level config (/etc/nix/machines, nix.conf substituters)
# is NOT managed here — that belongs in the nix repo via nix-darwin.
# This module provides the values; the nix repo consumes them.

{ lib, config, pkgs, ... }:

let
  cfg = config.blackmatter.components.ro;
  inherit (lib) mkEnableOption mkOption types mkIf mkMerge optional;
in
{
  options.blackmatter.components.ro = {
    enable = mkEnableOption "ro build platform integration";

    builder = {
      enable = mkEnableOption "remote builder connection to ro cluster";

      endpoint = mkOption {
        type = types.str;
        default = "nix-builder.nix-builder.svc";
        description = "SSH endpoint for the nix remote builder";
      };

      port = mkOption {
        type = types.int;
        default = 22;
        description = "SSH port for the builder";
      };

      sshKeyPath = mkOption {
        type = types.str;
        default = "~/.ssh/ro-builder-key";
        description = "Path to SSH private key for builder auth";
      };

      user = mkOption {
        type = types.str;
        default = "root";
        description = "SSH user for the builder";
      };

      maxJobs = mkOption {
        type = types.int;
        default = 8;
        description = "Max concurrent build jobs on the remote builder";
      };

      speedFactor = mkOption {
        type = types.int;
        default = 1;
        description = "Speed factor for nix remote builder scheduling";
      };

      systems = mkOption {
        type = types.listOf types.str;
        default = [ "x86_64-linux" "aarch64-linux" ];
        description = "Supported build systems on the remote builder";
      };

      supportedFeatures = mkOption {
        type = types.listOf types.str;
        default = [ "nixos-test" "benchmark" "big-parallel" "kvm" ];
        description = "Supported features advertised by the builder";
      };
    };

    cache = {
      enable = mkEnableOption "Attic binary cache as nix substituter";

      endpoint = mkOption {
        type = types.str;
        default = "cache.ro.ben-kar.com";
        description = "Attic cache HTTP endpoint";
      };

      cacheName = mkOption {
        type = types.str;
        default = "main";
        description = "Attic cache name";
      };

      publicKey = mkOption {
        type = types.str;
        default = "";
        description = "Nix public signing key for the cache (trusted-public-keys entry)";
      };
    };

    # Read-only outputs for consumption by the nix repo.
    # These are NOT config options — they are computed values.
    _computed = {
      builderLine = mkOption {
        type = types.str;
        readOnly = true;
        description = "Builder line for /etc/nix/machines (consumed by nix repo)";
      };

      substituterUrl = mkOption {
        type = types.str;
        readOnly = true;
        description = "Substituter URL for nix.conf (consumed by nix repo)";
      };

      trustedPublicKey = mkOption {
        type = types.str;
        readOnly = true;
        description = "Trusted public key for nix.conf (consumed by nix repo)";
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    # Install attic-client when cache is enabled
    (mkIf cfg.cache.enable {
      home.packages = [ pkgs.attic-client ];
    })

    # Configure SSH for the builder endpoint
    (mkIf cfg.builder.enable {
      programs.ssh.matchBlocks."ro-builder" = {
        hostname = cfg.builder.endpoint;
        port = cfg.builder.port;
        user = cfg.builder.user;
        identityFile = cfg.builder.sshKeyPath;
        extraOptions = {
          StrictHostKeyChecking = "accept-new";
        };
      };
    })

    # Compute derived values for consumption by the nix repo
    {
      blackmatter.components.ro._computed = {
        # Format: ssh://user@host system key maxJobs speedFactor features
        # See: https://nixos.org/manual/nix/stable/advanced-topics/distributed-builds
        builderLine = let
          systems = lib.concatStringsSep "," cfg.builder.systems;
          features = lib.concatStringsSep "," cfg.builder.supportedFeatures;
        in
          "ssh://${cfg.builder.user}@${cfg.builder.endpoint}"
          + " ${systems}"
          + " ${cfg.builder.sshKeyPath}"
          + " ${toString cfg.builder.maxJobs}"
          + " ${toString cfg.builder.speedFactor}"
          + " ${features}";

        substituterUrl =
          "https://${cfg.cache.endpoint}/${cfg.cache.cacheName}";

        trustedPublicKey = cfg.cache.publicKey;
      };
    }
  ]);
}
