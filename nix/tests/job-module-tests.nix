# Nix unit tests for tatara job module.
#
# Tests the NixOS module system types and normalizeJob function
# from nix/job-module.nix.
#
# Run with:
#   nix eval -f nix/tests/job-module-tests.nix
#
# Returns [] if all tests pass, or a list of failure descriptions.

{ lib ? (import <nixpkgs> { }).lib }:

let
  jobModule = import ../job-module.nix { inherit lib; };

  # ── Test: minimal service job ──

  minimalJob = {
    type = "service";
    groups = {
      main = {
        count = 1;
        tasks = {
          app = {
            driver = "exec";
            config = {
              command = "echo";
              args = [ "hello" ];
            };
            env = { };
            resources = {
              cpu_mhz = 100;
              memory_mb = 64;
            };
            health_checks = [ ];
          };
        };
        restart_policy = { };
        resources = { };
      };
    };
    constraints = [ ];
    meta = { };
  };

  normalizedMinimal = jobModule.normalizeJob "minimal" minimalJob;

  # ── Test: all driver types ──

  execTask = {
    driver = "exec";
    config = {
      command = "ls";
    };
    env = { };
    resources = {
      cpu_mhz = 100;
      memory_mb = 64;
    };
    health_checks = [ ];
  };

  ociTask = {
    driver = "oci";
    config = {
      image = "nginx:latest";
    };
    env = { };
    resources = {
      cpu_mhz = 200;
      memory_mb = 128;
    };
    health_checks = [ ];
  };

  nixTask = {
    driver = "nix";
    config = {
      flake_ref = "github:user/app";
    };
    env = { };
    resources = {
      cpu_mhz = 500;
      memory_mb = 256;
    };
    health_checks = [ ];
  };

  allDriversJob = {
    type = "service";
    groups = {
      mixed = {
        count = 1;
        tasks = {
          exec-task = execTask;
          oci-task = ociTask;
          nix-task = nixTask;
        };
        restart_policy = { };
        resources = { };
      };
    };
    constraints = [ ];
    meta = { };
  };

  normalizedAllDrivers = jobModule.normalizeJob "all-drivers" allDriversJob;

  # ── Test: job with constraints ──

  constrainedJob = {
    type = "service";
    groups = {
      main = {
        count = 1;
        tasks = {
          app = execTask;
        };
        restart_policy = { };
        resources = { };
      };
    };
    constraints = [
      {
        attribute = "os";
        operator = "=";
        value = "linux";
      }
      {
        attribute = "arch";
        operator = "=";
        value = "x86_64";
      }
    ];
    meta = { };
  };

  normalizedConstrained = jobModule.normalizeJob "constrained" constrainedJob;

  # ── Test: system job ──

  systemJob = {
    type = "system";
    groups = {
      agent = {
        count = 1;
        tasks = {
          daemon = execTask;
        };
        restart_policy = {
          mode = "always";
          attempts = 0;
        };
        resources = { };
      };
    };
    constraints = [ ];
    meta = {
      purpose = "monitoring";
    };
  };

  normalizedSystem = jobModule.normalizeJob "system-agent" systemJob;

  # ── Test: metadata preservation ──

  metaJob = {
    type = "service";
    groups = {
      main = {
        count = 1;
        tasks = {
          app = execTask;
        };
        restart_policy = { };
        resources = { };
      };
    };
    constraints = [ ];
    meta = {
      team = "platform";
      environment = "staging";
      version = "1.2.3";
      forge = "true";
    };
  };

  normalizedMeta = jobModule.normalizeJob "meta-job" metaJob;

in
lib.runTests {
  # ── Minimal job ──

  testMinimalJobId = {
    expr = normalizedMinimal.id;
    expected = "minimal";
  };

  testMinimalJobType = {
    expr = normalizedMinimal.job_type;
    expected = "service";
  };

  testMinimalJobGroupCount = {
    expr = builtins.length normalizedMinimal.groups;
    expected = 1;
  };

  testMinimalJobGroupName = {
    expr = (builtins.head normalizedMinimal.groups).name;
    expected = "main";
  };

  testMinimalJobTaskCount = {
    expr = builtins.length (builtins.head normalizedMinimal.groups).tasks;
    expected = 1;
  };

  testMinimalJobTaskDriver = {
    expr = (builtins.head (builtins.head normalizedMinimal.groups).tasks).driver;
    expected = "exec";
  };

  testMinimalJobEmptyConstraints = {
    expr = builtins.length normalizedMinimal.constraints;
    expected = 0;
  };

  # ── All driver types ──

  testAllDriversGroupTaskCount = {
    expr = builtins.length (builtins.head normalizedAllDrivers.groups).tasks;
    expected = 3;
  };

  # ── Constrained job ──

  testConstrainedJobHasConstraints = {
    expr = builtins.length normalizedConstrained.constraints;
    expected = 2;
  };

  testConstraintOsAttribute = {
    expr = (builtins.head normalizedConstrained.constraints).attribute;
    expected = "os";
  };

  # ── System job ──

  testSystemJobType = {
    expr = normalizedSystem.job_type;
    expected = "system";
  };

  testSystemJobId = {
    expr = normalizedSystem.id;
    expected = "system-agent";
  };

  testSystemJobMeta = {
    expr = normalizedSystem.meta.purpose;
    expected = "monitoring";
  };

  # ── Metadata preservation ──

  testMetaTeam = {
    expr = normalizedMeta.meta.team;
    expected = "platform";
  };

  testMetaEnvironment = {
    expr = normalizedMeta.meta.environment;
    expected = "staging";
  };

  testMetaVersion = {
    expr = normalizedMeta.meta.version;
    expected = "1.2.3";
  };

  testMetaForge = {
    expr = normalizedMeta.meta.forge;
    expected = "true";
  };
}
