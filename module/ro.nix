# ro (炉) — Nix build platform client integration
#
# The client only needs one thing: the ro API endpoint.
# Everything else (cache URL, public keys, builder config) is
# served by the API and consumed automatically.
#
# The ro API exposes GET /config which returns:
#   { substituters: [...], trusted_public_keys: [...], builder: { ... } }
#
# System-level nix config (/etc/nix/machines, nix.conf substituters)
# is managed by the nix repo via nix-darwin. This module provides
# computed values that the nix repo consumes.

{ lib, config, pkgs, ... }:

let
  cfg = config.blackmatter.components.ro;
  inherit (lib) mkEnableOption mkOption types mkIf;
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

    # Read-only computed outputs for consumption by the nix repo.
    # These derive everything from apiEndpoint following ro's URL conventions.
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

  config = mkIf cfg.enable {
    # Install the ro client tools
    home.packages = [
      pkgs.attic-client
      # ro CLI is built from this repo's flake:
      #   pkgs.ro  (from tatara overlay)
      # Add when available: pkgs.ro
    ];

    # Write shikumi config file for the ro CLI
    # This is the Nix → YAML → Rust bridge
    xdg.configFile."ro/ro.yaml".text = builtins.toJSON {
      api_endpoint = cfg.apiEndpoint;
    };

    # Computed values — derived from the single apiEndpoint.
    # The ro platform uses a consistent subdomain structure:
    #   api.ro.<domain>   → tatara-operator API
    #   cache.ro.<domain> → Attic binary cache
    #   webhook.ro.<domain> → GitHub webhook receiver
    #
    # The client only needs the cache URL as a substituter.
    # Build submission is via kubectl (NixBuild CRDs) or the API.
    blackmatter.components.ro._computed = let
      # Derive cache URL from API URL: api.ro.X → cache.ro.X
      cacheUrl = builtins.replaceStrings [ "api." ] [ "cache." ] cfg.apiEndpoint;
    in {
      substituterUrl = "${cacheUrl}/main";
      configEndpoint = "${cfg.apiEndpoint}/config";
    };
  };
}
