# Tatara Forge Library
#
# Provides utilities for creating, evaluating, and validating
# tatara workload packages ("forges") — the Helm-chart equivalent
# for tatara's Nix-native orchestrator.
#
# A forge is a flake that exports:
#   - tataraModules.<name>  — NixOS modules for typed job configuration
#   - tataraJobs.<system>.<name> — Ready-to-submit normalized job specs
#   - tataraMeta — Package metadata (name, version, description)

{ lib ? (import <nixpkgs> {}).lib }:

let
  inherit (lib) mkOption types mkEnableOption mkIf mapAttrsToList;

  jobModule = import ../job-module.nix { inherit lib; };

  # ── Core: mkJobModule ──
  #
  # Create a typed NixOS module for a tatara job.
  # This produces a module with `options.tatara.jobs.<name>` that can be
  # imported and configured by consumers.
  #
  # Usage:
  #   mkJobModule {
  #     name = "myapp";
  #     driver = "nix";
  #     defaults = {
  #       groups.web.count = 3;
  #       groups.web.tasks.server.config.flake_ref = "github:me/myapp";
  #     };
  #   }
  #
  mkJobModule =
    {
      name,
      driver ? "nix",
      defaults ? { },
      description ? "tatara job: ${name}",
    }:
    { config, lib, ... }:
    {
      options.tatara.jobs.${name} = {
        enable = mkEnableOption description;

        jobType = mkOption {
          type = types.enum [
            "service"
            "batch"
            "system"
          ];
          default = defaults.type or "service";
          description = "Job type (service, batch, or system)";
        };

        groups = mkOption {
          type = types.attrsOf (jobModule.jobType.nestedTypes.elemType or types.attrs);
          default = defaults.groups or { };
          description = "Task groups";
        };

        constraints = mkOption {
          type = types.listOf types.attrs;
          default = defaults.constraints or [ ];
          description = "Node placement constraints";
        };

        meta = mkOption {
          type = types.attrsOf types.str;
          default = defaults.meta or { };
          description = "User metadata";
        };

        values = mkOption {
          type = types.attrs;
          default = { };
          description = "Override values (like Helm values.yaml)";
        };
      };

      config = mkIf config.tatara.jobs.${name}.enable {
        # The module system handles merging — consumers just set the options
      };
    };

  # ── evalForge ──
  #
  # Evaluate a forge flake path and return normalized job specs.
  # Takes a path to a forge (flake directory) and optional overrides.
  #
  # Returns: { jobs = { <name> = <normalized-job-spec>; ... }; meta = { ... }; }
  #
  evalForge =
    forgePath: overrides:
    let
      forgeFlake = builtins.getFlake (toString forgePath);

      # Get the current system
      system = builtins.currentSystem or "x86_64-linux";

      # Extract job specs from standard outputs
      jobs = forgeFlake.tataraJobs.${system} or { };

      # Extract metadata
      meta = forgeFlake.tataraMeta or {
        name = "unknown";
        version = "0.0.0";
      };

      # Apply overrides to each job
      applyOverrides =
        jobSpec:
        let
          jobOverrides =
            if overrides ? ${jobSpec.id or "default"} then
              overrides.${jobSpec.id or "default"}
            else if overrides != { } then
              overrides
            else
              { };
        in
        lib.recursiveUpdate jobSpec jobOverrides;
    in
    {
      inherit meta;
      jobs = builtins.mapAttrs (_name: applyOverrides) jobs;
    };

  # ── validateForge ──
  #
  # Validate that a forge directory has the expected structure.
  # Returns a list of errors (empty = valid).
  #
  validateForge =
    forgePath:
    let
      forgeFlake = builtins.getFlake (toString forgePath);
      system = builtins.currentSystem or "x86_64-linux";

      errors =
        (if !(forgeFlake ? tataraMeta) then [ "Missing 'tataraMeta' output" ] else [ ])
        ++ (
          if !(forgeFlake ? tataraJobs)
          then [ "Missing 'tataraJobs' output" ]
          else if !(forgeFlake.tataraJobs ? ${system})
          then [ "Missing 'tataraJobs.${system}' output" ]
          else [ ]
        )
        ++ (
          if (forgeFlake ? tataraMeta) then
            (if !(forgeFlake.tataraMeta ? name) then [ "tataraMeta missing 'name'" ] else [ ])
            ++ (
              if !(forgeFlake.tataraMeta ? version) then [ "tataraMeta missing 'version'" ] else [ ]
            )
          else
            [ ]
        );
    in
    {
      valid = errors == [ ];
      inherit errors;
    };

  # ── Forge template scaffolding ──
  #
  # Returns a string containing the content of a new forge's flake.nix.
  #
  forgeTemplate =
    name:
    ''
      {
        description = "${name} — tatara forge";

        inputs = {
          nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
          tatara.url = "github:pleme-io/tatara";
        };

        outputs = { self, nixpkgs, tatara, ... }:
        let
          systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
          forEachSystem = nixpkgs.lib.genAttrs systems;
        in
        {
          # Module for typed configuration
          tataraModules.${name} = tatara.lib.mkJobModule {
            name = "${name}";
            driver = "nix";
            defaults = {
              groups.main = {
                count = 1;
                tasks.app = {
                  driver = "nix";
                  config = {
                    flake_ref = "github:you/${name}";
                  };
                  resources = {
                    cpu_mhz = 500;
                    memory_mb = 256;
                  };
                };
              };
            };
          };

          # Ready-to-submit job specs per system
          tataraJobs = forEachSystem (system: {
            ${name} = tatara.lib.normalizeJob "${name}" {
              type = "service";
              groups.main = {
                count = 1;
                tasks.app = {
                  driver = "nix";
                  config = {
                    flake_ref = "github:you/${name}";
                  };
                  env = {};
                  resources = {
                    cpu_mhz = 500;
                    memory_mb = 256;
                  };
                  health_checks = [];
                };
                restart_policy = {};
                resources = {};
              };
              constraints = [];
              meta = {};
            };
          });

          # Forge metadata
          tataraMeta = {
            name = "${name}-forge";
            version = "1.0.0";
            description = "${name} workload for tatara";
          };
        };
      }
    '';

in
{
  inherit
    mkJobModule
    evalForge
    validateForge
    forgeTemplate
    ;
  # Re-export job module utilities
  inherit (jobModule) normalizeJob;
}
