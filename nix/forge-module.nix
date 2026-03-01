# Tatara Forge Module Extension
#
# Extends the base job-module.nix with forge-specific options:
#   - forge.name, forge.version, forge.description
#   - forge.values — typed override interface (like Helm values.yaml)
#   - forge.dependencies — other forges this depends on
#
# Usage:
#   Import this module alongside job-module.nix to add forge metadata
#   to any tatara job specification.

{ lib ? (import <nixpkgs> {}).lib }:

let
  inherit (lib) mkOption types;

  forgeMetaType = types.submodule {
    options = {
      name = mkOption {
        type = types.str;
        description = "Forge package name";
      };

      version = mkOption {
        type = types.str;
        default = "0.0.0";
        description = "Forge version (semver)";
      };

      description = mkOption {
        type = types.str;
        default = "";
        description = "Human-readable description of this forge";
      };
    };
  };

  forgeValuesType = types.submodule {
    options = {
      # The values interface is intentionally unstructured (attrs)
      # to allow forges to define their own schema.
      # Consumers override values via `--set` or Nix module system.
      overrides = mkOption {
        type = types.attrs;
        default = {};
        description = "Override values applied to the job spec (like Helm values.yaml)";
      };
    };
  };

  forgeDependencyType = types.submodule {
    options = {
      name = mkOption {
        type = types.str;
        description = "Name of the dependency forge";
      };

      flake_ref = mkOption {
        type = types.str;
        description = "Flake reference for the dependency";
      };

      version = mkOption {
        type = types.str;
        default = "*";
        description = "Version constraint for the dependency";
      };
    };
  };

  forgeType = types.submodule {
    options = {
      meta = mkOption {
        type = forgeMetaType;
        description = "Forge package metadata";
      };

      values = mkOption {
        type = forgeValuesType;
        default = {};
        description = "Configurable values for this forge";
      };

      dependencies = mkOption {
        type = types.listOf forgeDependencyType;
        default = [];
        description = "Other forges this forge depends on";
      };
    };
  };

in {
  inherit forgeType forgeMetaType forgeValuesType forgeDependencyType;
}
