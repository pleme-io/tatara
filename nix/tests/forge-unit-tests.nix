# Nix unit tests for tatara forge library.
#
# Run with:
#   nix eval -f nix/tests/forge-unit-tests.nix
#
# Returns [] if all tests pass, or a list of failure descriptions.

{ lib ? (import <nixpkgs> { }).lib }:

let
  forge = import ../lib/forge.nix { inherit lib; };

  # ── forgeTemplate tests ──

  templateResult = forge.forgeTemplate "myapp";

  # ── normalizeJob tests ──

  sampleJob = {
    type = "service";
    groups = {
      web = {
        count = 3;
        tasks = {
          app = {
            driver = "nix";
            config = {
              flake_ref = "github:user/myapp";
            };
            env = {
              PORT = "8080";
            };
            resources = {
              cpu_mhz = 500;
              memory_mb = 256;
            };
            health_checks = [ ];
          };
        };
        restart_policy = { };
        resources = {
          cpu_mhz = 500;
          memory_mb = 256;
        };
      };
    };
    constraints = [
      {
        attribute = "os";
        operator = "=";
        value = "linux";
      }
    ];
    meta = {
      team = "platform";
      env = "production";
    };
  };

  normalized = forge.normalizeJob "myapp" sampleJob;

  # ── Batch job tests ──

  batchJob = {
    type = "batch";
    groups = {
      main = {
        count = 1;
        tasks = {
          worker = {
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
        restart_policy = {
          mode = "never";
        };
        resources = { };
      };
    };
    constraints = [ ];
    meta = { };
  };

  normalizedBatch = forge.normalizeJob "batch-worker" batchJob;

  # ── Multi-group job tests ──

  multiGroupJob = {
    type = "service";
    groups = {
      frontend = {
        count = 2;
        tasks = {
          web = {
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
        };
        restart_policy = { };
        resources = { };
      };
      backend = {
        count = 3;
        tasks = {
          api = {
            driver = "nix";
            config = {
              flake_ref = "github:user/api";
            };
            env = {
              DATABASE_URL = "postgres://localhost/db";
            };
            resources = {
              cpu_mhz = 1000;
              memory_mb = 512;
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

  normalizedMulti = forge.normalizeJob "multi-app" multiGroupJob;

in
lib.runTests {
  # ── normalizeJob: basic fields ──

  testNormalizeJobSetsId = {
    expr = normalized.id;
    expected = "myapp";
  };

  testNormalizeJobSetsType = {
    expr = normalized.job_type;
    expected = "service";
  };

  testNormalizeJobPreservesConstraints = {
    expr = builtins.length normalized.constraints;
    expected = 1;
  };

  testNormalizeJobConstraintAttribute = {
    expr = (builtins.head normalized.constraints).attribute;
    expected = "os";
  };

  testNormalizeJobConstraintValue = {
    expr = (builtins.head normalized.constraints).value;
    expected = "linux";
  };

  testNormalizeJobPreservesMeta = {
    expr = normalized.meta.team;
    expected = "platform";
  };

  testNormalizeJobMetaEnv = {
    expr = normalized.meta.env;
    expected = "production";
  };

  # ── normalizeJob: group structure ──

  testNormalizeJobGroupCount = {
    expr = builtins.length normalized.groups;
    expected = 1;
  };

  testNormalizeJobGroupName = {
    expr = (builtins.head normalized.groups).name;
    expected = "web";
  };

  testNormalizeJobGroupInstanceCount = {
    expr = (builtins.head normalized.groups).count;
    expected = 3;
  };

  # ── normalizeJob: task structure ──

  testNormalizeJobTaskCount = {
    expr = builtins.length (builtins.head normalized.groups).tasks;
    expected = 1;
  };

  testNormalizeJobTaskName = {
    expr = (builtins.head (builtins.head normalized.groups).tasks).name;
    expected = "app";
  };

  testNormalizeJobTaskDriver = {
    expr = (builtins.head (builtins.head normalized.groups).tasks).driver;
    expected = "nix";
  };

  testNormalizeJobTaskEnv = {
    expr = (builtins.head (builtins.head normalized.groups).tasks).env.PORT;
    expected = "8080";
  };

  testNormalizeJobTaskResources = {
    expr = (builtins.head (builtins.head normalized.groups).tasks).resources.cpu_mhz;
    expected = 500;
  };

  # ── normalizeJob: batch job ──

  testBatchJobType = {
    expr = normalizedBatch.job_type;
    expected = "batch";
  };

  testBatchJobId = {
    expr = normalizedBatch.id;
    expected = "batch-worker";
  };

  # ── normalizeJob: multi-group ──

  testMultiGroupCount = {
    expr = builtins.length normalizedMulti.groups;
    expected = 2;
  };

  testMultiGroupJobId = {
    expr = normalizedMulti.id;
    expected = "multi-app";
  };

  # ── forgeTemplate ──

  testForgeTemplateContainsName = {
    expr = builtins.match ".*myapp.*" templateResult != null;
    expected = true;
  };

  testForgeTemplateContainsTataraMeta = {
    expr = builtins.match ".*tataraMeta.*" templateResult != null;
    expected = true;
  };

  testForgeTemplateContainsTataraJobs = {
    expr = builtins.match ".*tataraJobs.*" templateResult != null;
    expected = true;
  };

  testForgeTemplateContainsTataraModules = {
    expr = builtins.match ".*tataraModules.*" templateResult != null;
    expected = true;
  };

  testForgeTemplateContainsTataraInput = {
    expr = builtins.match ".*tatara.url.*" templateResult != null;
    expected = true;
  };

  testForgeTemplateContainsNixpkgsInput = {
    expr = builtins.match ".*nixpkgs.url.*" templateResult != null;
    expected = true;
  };
}
