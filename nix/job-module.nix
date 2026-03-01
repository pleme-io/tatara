# NixOS module system for tatara job specifications.
#
# Usage:
#   nix eval --json -f jobs/my-job.nix
#
# This module provides type-safe job spec construction using the NixOS module system.

{ lib ? (import <nixpkgs> {}).lib }:

let
  inherit (lib) mkOption types;

  resourceType = types.submodule {
    options = {
      cpu_mhz = mkOption {
        type = types.int;
        default = 0;
        description = "Requested CPU in MHz";
      };
      memory_mb = mkOption {
        type = types.int;
        default = 0;
        description = "Requested memory in MB";
      };
    };
  };

  healthCheckType = types.submodule {
    options = {
      http = mkOption {
        type = types.nullOr (types.submodule {
          options = {
            port = mkOption { type = types.int; };
            path = mkOption { type = types.str; default = "/healthz"; };
            interval_secs = mkOption { type = types.int; default = 10; };
            timeout_secs = mkOption { type = types.int; default = 5; };
          };
        });
        default = null;
      };
      exec = mkOption {
        type = types.nullOr (types.submodule {
          options = {
            command = mkOption { type = types.str; };
            interval_secs = mkOption { type = types.int; default = 10; };
            timeout_secs = mkOption { type = types.int; default = 5; };
          };
        });
        default = null;
      };
      tcp = mkOption {
        type = types.nullOr (types.submodule {
          options = {
            port = mkOption { type = types.int; };
            interval_secs = mkOption { type = types.int; default = 10; };
            timeout_secs = mkOption { type = types.int; default = 5; };
          };
        });
        default = null;
      };
    };
  };

  taskType = types.submodule {
    options = {
      driver = mkOption {
        type = types.enum [ "exec" "oci" "nix" ];
        description = "Task driver";
      };
      config = mkOption {
        type = types.attrs;
        description = "Driver-specific configuration";
      };
      env = mkOption {
        type = types.attrsOf types.str;
        default = {};
        description = "Environment variables";
      };
      resources = mkOption {
        type = resourceType;
        default = {};
      };
      health_checks = mkOption {
        type = types.listOf healthCheckType;
        default = [];
      };
    };
  };

  restartPolicyType = types.submodule {
    options = {
      mode = mkOption {
        type = types.enum [ "on_failure" "always" "never" ];
        default = "on_failure";
      };
      attempts = mkOption {
        type = types.int;
        default = 3;
      };
      interval_secs = mkOption {
        type = types.int;
        default = 300;
      };
      delay_secs = mkOption {
        type = types.int;
        default = 5;
      };
    };
  };

  taskGroupType = types.submodule {
    options = {
      count = mkOption {
        type = types.int;
        default = 1;
        description = "Number of instances";
      };
      tasks = mkOption {
        type = types.attrsOf taskType;
        description = "Tasks in this group";
      };
      restart_policy = mkOption {
        type = restartPolicyType;
        default = {};
      };
      resources = mkOption {
        type = resourceType;
        default = {};
      };
    };
  };

  constraintType = types.submodule {
    options = {
      attribute = mkOption { type = types.str; };
      operator = mkOption { type = types.str; default = "="; };
      value = mkOption { type = types.str; };
    };
  };

  jobType = types.submodule {
    options = {
      type = mkOption {
        type = types.enum [ "service" "batch" "system" ];
        default = "service";
      };
      groups = mkOption {
        type = types.attrsOf taskGroupType;
        description = "Task groups";
      };
      constraints = mkOption {
        type = types.listOf constraintType;
        default = [];
      };
      meta = mkOption {
        type = types.attrsOf types.str;
        default = {};
      };
    };
  };

  # Convert the NixOS module attrset into the flat JSON format tatara expects
  normalizeJob = name: job: {
    id = name;
    job_type = job.type;
    groups = lib.mapAttrsToList (groupName: group: {
      name = groupName;
      count = group.count;
      tasks = lib.mapAttrsToList (taskName: task: {
        name = taskName;
        driver = task.driver;
        config = { type = task.driver; } // task.config;
        env = task.env;
        resources = task.resources;
        health_checks = builtins.filter (hc: hc.http != null || hc.exec != null || hc.tcp != null) task.health_checks;
      }) group.tasks;
      restart_policy = group.restart_policy;
      resources = group.resources;
    }) job.groups;
    constraints = job.constraints;
    meta = job.meta;
  };

in {
  inherit jobType normalizeJob;
}
