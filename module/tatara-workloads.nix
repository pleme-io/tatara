# Declarative workload definitions for tatara.
#
# Define workloads in Nix, submitted to tatara on activation.
# For production, use the Source reconciler (GitOps) instead.
#
# Usage:
#   services.tatara.workloads.my-service = {
#     enable = true;
#     groups.main = {
#       count = 1;
#       tasks.app = {
#         driver = "nix";
#         flakeRef = "github:pleme-io/my-service";
#         resources = { cpuMhz = 500; memoryMb = 256; };
#       };
#     };
#   };
{ hmHelpers }:
{ config, lib, pkgs, ... }:

let
  cfg = config.services.tatara;
  inherit (lib) mkEnableOption mkOption types mkIf mapAttrs' mapAttrsToList concatStringsSep;

  taskType = types.submodule {
    options = {
      driver = mkOption {
        type = types.enum [ "exec" "nix" "oci" "nix_build" "kasou" "kube" ];
        default = "nix";
        description = "Execution driver";
      };
      flakeRef = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Nix flake reference (for nix/nix_build drivers)";
      };
      command = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Command to execute (for exec driver)";
      };
      args = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Arguments";
      };
      image = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "OCI image (for oci driver)";
      };
      env = mkOption {
        type = types.attrsOf types.str;
        default = {};
        description = "Environment variables";
      };
      resources = {
        cpuMhz = mkOption {
          type = types.int;
          default = 0;
          description = "CPU in MHz";
        };
        memoryMb = mkOption {
          type = types.int;
          default = 0;
          description = "Memory in MB";
        };
      };
      healthChecks = mkOption {
        type = types.listOf (types.attrsOf types.anything);
        default = [];
        description = "Health check definitions";
      };
    };
  };

  groupType = types.submodule {
    options = {
      count = mkOption {
        type = types.int;
        default = 1;
        description = "Number of instances";
      };
      tasks = mkOption {
        type = types.attrsOf taskType;
        default = {};
        description = "Tasks in this group";
      };
      serviceName = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Register in service catalog under this name";
      };
      restartPolicy = {
        mode = mkOption {
          type = types.enum [ "on_failure" "always" "never" ];
          default = "on_failure";
        };
        attempts = mkOption {
          type = types.int;
          default = 3;
        };
      };
    };
  };

  workloadType = types.submodule {
    options = {
      enable = mkEnableOption "this workload";
      jobType = mkOption {
        type = types.enum [ "service" "batch" "system" ];
        default = "service";
      };
      groups = mkOption {
        type = types.attrsOf groupType;
        default = {};
      };
      constraints = mkOption {
        type = types.listOf (types.attrsOf types.str);
        default = [];
      };
      meta = mkOption {
        type = types.attrsOf types.str;
        default = {};
      };
    };
  };

  # Convert a Nix workload definition to a tatara job JSON spec.
  workloadToJson = name: wl: let
    taskToJson = tname: task: {
      name = tname;
      driver = task.driver;
      config =
        if task.driver == "nix" && task.flakeRef != null then
          { type = "nix"; flake_ref = task.flakeRef; args = task.args; }
        else if task.driver == "exec" && task.command != null then
          { type = "exec"; command = task.command; args = task.args; }
        else if task.driver == "oci" && task.image != null then
          { type = "oci"; image = task.image; }
        else
          { type = task.driver; };
      env = task.env;
      resources = {
        cpu_mhz = task.resources.cpuMhz;
        memory_mb = task.resources.memoryMb;
      };
      health_checks = task.healthChecks;
    };
    groupToJson = gname: group: {
      name = gname;
      count = group.count;
      tasks = mapAttrsToList taskToJson group.tasks;
      restart_policy = {
        mode = group.restartPolicy.mode;
        attempts = group.restartPolicy.attempts;
      };
      service_name = group.serviceName;
    };
  in builtins.toJSON {
    id = name;
    job_type = wl.jobType;
    groups = mapAttrsToList groupToJson wl.groups;
    constraints = wl.constraints;
    meta = wl.meta;
  };

in {
  options.services.tatara.workloads = mkOption {
    type = types.attrsOf workloadType;
    default = {};
    description = "Declarative workload definitions submitted to tatara on activation";
  };

  config = mkIf (cfg.workloads != {} && cfg.server.enable) {
    # Write job specs as JSON files for the activation hook.
    xdg.configFile = lib.mapAttrs' (name: wl:
      lib.nameValuePair "tatara/workloads/${name}.json" {
        text = workloadToJson name wl;
      }
    ) (lib.filterAttrs (_: wl: wl.enable) cfg.workloads);
  };
}
