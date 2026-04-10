# ro (炉) — Nix build platform client integration
#
# The client only needs one thing: the ro API endpoint.
# Everything else (cache URL, public keys, builder config) is
# served by the API and consumed automatically.
#
# The ro API exposes GET /config which returns:
#   { substituters: [...], trusted_public_keys: [...], builder: { ... } }
#
# This module:
#   1. Writes ~/.config/ro/ro.yaml (shikumi config)
#   2. Bootstraps ~/.config/nix/ro.conf (substituters from Nix options)
#   3. Runs `ro refresh` periodically to keep config live
#   4. System-level nix.conf is managed by nix-darwin — computed values
#      are exported for the nix repo to consume.

{ hmHelpers }:
{ lib, config, pkgs, ... }:

let
  cfg = config.blackmatter.components.ro;
  inherit (lib) mkEnableOption mkOption types mkIf mkMerge;
  isDarwin = pkgs.stdenv.isDarwin;
in
{
  options.blackmatter.components.ro = {
    enable = mkEnableOption "ro (炉) Nix build platform";

    apiEndpoint = mkOption {
      type = types.str;
      default = "https://api.ro.ben-kar.com";
      description = ''
        The ro platform API endpoint. This is the ONLY configuration
        the client needs. All other settings (cache URLs, public keys,
        builder endpoints) are served by the API.
      '';
    };

    refreshInterval = mkOption {
      type = types.int;
      default = 3600;
      description = "Interval in seconds for periodic `ro refresh` (default: 1h)";
    };

    # Read-only computed outputs for consumption by the nix repo.
    _computed = {
      substituterUrl = mkOption {
        type = types.str;
        readOnly = true;
        description = "Attic cache substituter URL (derived from apiEndpoint)";
      };

      configEndpoint = mkOption {
        type = types.str;
        readOnly = true;
        description = "API endpoint for client config discovery";
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    {
      # Install the ro client tools
      home.packages = [
        pkgs.attic-client
      ] ++ lib.optional (pkgs ? ro) pkgs.ro;

      # Write shikumi config file for the ro CLI
      xdg.configFile."ro/ro.yaml".text = builtins.toJSON {
        api_endpoint = cfg.apiEndpoint;
      };

      # Bootstrap nix substituter from Nix options.
      # This ensures the Attic cache is used immediately after rebuild,
      # even before `ro refresh` has ever run. The ro CLI will overwrite
      # this file with live data on first refresh.
      xdg.configFile."nix/ro.conf".text = ''
        # Managed by ro (炉) — bootstrap from Nix module
        # Run `ro refresh` to update with live platform config
        extra-substituters = ${cfg._computed.substituterUrl}
      '';

      # Computed values
      blackmatter.components.ro._computed = let
        cacheUrl = builtins.replaceStrings [ "api." ] [ "cache." ] cfg.apiEndpoint;
      in {
        substituterUrl = "${cacheUrl}/main";
        configEndpoint = "${cfg.apiEndpoint}/config";
      };
    }

    # Periodic refresh — macOS (launchd)
    (mkIf isDarwin (hmHelpers.mkLaunchdPeriodicTask {
      name = "ro-refresh";
      label = "io.pleme.ro.refresh";
      command = "${if pkgs ? ro then pkgs.ro else pkgs.coreutils}/bin/${if pkgs ? ro then "ro" else "true"}";
      args = if pkgs ? ro then [ "refresh" ] else [];
      interval = cfg.refreshInterval;
      logDir = "${config.xdg.dataHome}/ro/logs";
    }))

    # Periodic refresh — Linux (systemd timer)
    (mkIf (!isDarwin) (hmHelpers.mkSystemdPeriodicTask {
      name = "ro-refresh";
      description = "ro (炉) platform config refresh";
      command = "${if pkgs ? ro then pkgs.ro else pkgs.coreutils}/bin/${if pkgs ? ro then "ro" else "true"}";
      args = if pkgs ? ro then [ "refresh" ] else [];
      interval = cfg.refreshInterval;
    }))
  ]);
}
