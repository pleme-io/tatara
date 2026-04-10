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
  in
    # Merge tool + operator outputs. Operator packages/apps are namespaced
    # under "operator-*" to avoid colliding with the CLI tool.
    toolOutputs
    // {
      homeManagerModules.default = import ./module {
        hmHelpers = import "${substrate}/lib/hm-service-helpers.nix" { lib = nixpkgs.lib; };
      };

      lib = import ./nix/lib/forge.nix { lib = nixpkgs.lib; };

      # Operator outputs — access via tatara.packages.${system}.operator-*
      packages = nixpkgs.lib.genAttrs
        [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ]
        (system:
          (toolOutputs.packages.${system} or {})
          // (let op = operatorOutputs.packages.${system} or {}; in {
            operator-image-amd64 = op.dockerImage-amd64 or null;
            operator = op.tatara-operator or op.default or null;
          })
        );

      # Operator release app
      apps = nixpkgs.lib.genAttrs
        [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ]
        (system:
          (toolOutputs.apps.${system} or {})
          // {
            release-operator = (operatorOutputs.apps.${system} or {}).release or {
              type = "app";
              program = "echo 'operator release not available on ${system}'";
            };
          }
        );
    };
}
