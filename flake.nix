{
  description = "tatara — Nix-native workload orchestrator";

  nixConfig = {
    allow-import-from-derivation = true;
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    crate2nix.url = "github:nix-community/crate2nix";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    forge = {
      url = "github:pleme-io/forge";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crate2nix,
    flake-utils,
    substrate,
    forge,
    devenv,
  }: let
    # CLI tool release (tatara binary — 4-target GitHub releases)
    toolOutputs = (import "${substrate}/lib/rust-tool-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils devenv;
    }) {
      toolName = "tatara";
      src = self;
      repo = "pleme-io/tatara";
    };

    # Operator Docker image (tatara-operator — K8s deployment via substrate)
    operatorOutputs = (import "${substrate}/lib/rust-tool-image-flake.nix" {
      inherit nixpkgs crate2nix flake-utils forge devenv;
    }) {
      toolName = "tatara-operator";
      packageName = "tatara-operator";
      src = self;
      repo = "pleme-io/tatara-operator";
      architectures = [ "amd64" ];
      env = [
        "NATS_URL=nats://nats.nats.svc:4222"
      ];
    };

    # Reconciler Docker image (tatara-reconciler — the FluxCD-adjacent K8s
    # controller that reconciles Process CRDs as Unix processes).
    reconcilerOutputs = (import "${substrate}/lib/rust-tool-image-flake.nix" {
      inherit nixpkgs crate2nix flake-utils forge devenv;
    }) {
      toolName = "tatara-reconciler";
      packageName = "tatara-reconciler";
      src = self;
      repo = "pleme-io/tatara-reconciler";
      architectures = [ "amd64" ];
      env = [];
    };

    # tatara-init — PID 1 for tatara-os Linux guests. 4-target release build
    # per substrate convention so every guest arch (aarch64-linux,
    # x86_64-linux) gets a matching init binary.
    initOutputs = (import "${substrate}/lib/rust-workspace-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils devenv;
    }) {
      toolName = "tatara-init";
      packageName = "tatara-init";
      src = self;
      repo = "pleme-io/tatara-init";
    };

    # tatara-boot-gen — Lisp-file → bootable-artifact-tree generator. Darwin
    # host side (runs as part of `services.tatara-os-vm` activation), so
    # aarch64-darwin is the practical target but we build for all four to
    # stay consistent with the substrate convention.
    bootGenOutputs = (import "${substrate}/lib/rust-workspace-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils devenv;
    }) {
      toolName = "tatara-boot-gen";
      packageName = "tatara-vm";
      src = self;
      repo = "pleme-io/tatara-boot-gen";
    };

    # tatara-terreiro — CLI for sealed Lisp virtual environments.
    # Sub-commands: new / id / seal / load / compile / eval / realize.
    # Nord-themed stderr via tatara-ui; pipeable stdout.
    terreiroOutputs = (import "${substrate}/lib/rust-workspace-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils devenv;
    }) {
      toolName = "tatara-terreiro";
      packageName = "tatara-terreiro";
      src = self;
      repo = "pleme-io/tatara-terreiro";
    };

    # tatara-vmctl — control plane for tatara-os guests.
    # Sub-commands: list / status / build / up / down / destroy / logs / ip / ssh.
    # Shares the tatara-vm crate with tatara-boot-gen; this output targets
    # the vmctl binary specifically so it ships as a standalone package.
    vmctlOutputs = (import "${substrate}/lib/rust-workspace-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils devenv;
    }) {
      toolName = "tatara-vmctl";
      packageName = "tatara-vm";
      src = self;
      repo = "pleme-io/tatara-vmctl";
    };

    # ── CI-replacement surface ─────────────────────────────────────────
    # `cargo run --bin tatara-check` runs the typed workspace coherence suite
    # driven by checks.lisp (CRD drift, YAML parse, Process round-trip, etc.).
    # `nix flake check` runs the pure-sandbox derivations below (helm lint
    # — the only check that genuinely needs the helm binary).

    helmLintCheck = system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in pkgs.runCommand "tatara-reconciler-helm-lint" {
      nativeBuildInputs = [ pkgs.kubernetes-helm ];
      src = ./chart/tatara-reconciler;
    } ''
      cp -r $src ./chart
      chmod -R u+w ./chart
      helm lint ./chart
      touch $out
    '';
  in
    # Merge tool + operator outputs. Operator packages/apps are namespaced
    # under "operator-*" to avoid colliding with the CLI tool.
    toolOutputs
    // {
      homeManagerModules.default = import ./module {
        hmHelpers = import "${substrate}/lib/hm-service-helpers.nix" { lib = nixpkgs.lib; };
      };

      lib = import ./nix/lib/forge.nix { lib = nixpkgs.lib; };

      # Operator + reconciler outputs — access via:
      #   tatara.packages.${system}.operator-image-amd64
      #   tatara.packages.${system}.reconciler-image-amd64
      #   tatara.packages.${system}.init             ← tatara-init (PID 1)
      packages = nixpkgs.lib.genAttrs
        [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ]
        (system:
          (toolOutputs.packages.${system} or {})
          // (let op = operatorOutputs.packages.${system} or {}; in {
            operator-image-amd64 = op.dockerImage-amd64 or null;
            operator = op.tatara-operator or op.default or null;
          })
          // (let rc = reconcilerOutputs.packages.${system} or {}; in {
            reconciler-image-amd64 = rc.dockerImage-amd64 or null;
            reconciler = rc.tatara-reconciler or rc.default or null;
          })
          // (let it = initOutputs.packages.${system} or {}; in {
            init = it.tatara-init or it.default or null;
          })
          // (let bg = bootGenOutputs.packages.${system} or {}; in {
            boot-gen = bg.tatara-boot-gen or bg.default or null;
          })
          // (let tr = terreiroOutputs.packages.${system} or {}; in {
            terreiro = tr.tatara-terreiro or tr.default or null;
          })
          // (let vc = vmctlOutputs.packages.${system} or {}; in {
            vmctl = vc.tatara-vmctl or vc.default or null;
          })
        );

      # Operator + reconciler release apps + workspace check.
      apps = nixpkgs.lib.genAttrs
        [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ]
        (system:
          (toolOutputs.apps.${system} or {})
          // {
            release-operator = (operatorOutputs.apps.${system} or {}).release or {
              type = "app";
              program = "echo 'operator release not available on ${system}'";
            };
            release-reconciler = (reconcilerOutputs.apps.${system} or {}).release or {
              type = "app";
              program = "echo 'reconciler release not available on ${system}'";
            };
          }
        );

      # Pure sandboxed checks — `nix flake check` runs these.
      # Everything that needs cargo lives in `cargo run --bin tatara-check`
      # (driven by checks.lisp at the workspace root).
      checks = nixpkgs.lib.genAttrs
        [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ]
        (system: {
          helm-lint = helmLintCheck system;
        });
    };
}
