{ hmHelpers }:
{ config, lib, pkgs, ... }:

let
  cfg = config.services.tatara;
  inherit (lib) mkEnableOption mkOption types mkIf mkMerge;
  isDarwin = pkgs.stdenv.isDarwin;
in
{
  options.services.tatara = {
    server = {
      enable = mkEnableOption "tatara server";

      httpAddr = mkOption {
        type = types.str;
        default = "127.0.0.1:4646";
        description = "HTTP + GraphQL listen address";
      };

      grpcAddr = mkOption {
        type = types.str;
        default = "127.0.0.1:4647";
        description = "gRPC listen address";
      };

      logLevel = mkOption {
        type = types.str;
        default = "info";
        description = "Log level (trace, debug, info, warn, error)";
      };

      stateDir = mkOption {
        type = types.str;
        default = "~/.local/share/tatara/server";
        description = "Directory for persistent state";
      };

      evalIntervalSecs = mkOption {
        type = types.int;
        default = 1;
        description = "Scheduler evaluation interval in seconds";
      };

      heartbeatGraceSecs = mkOption {
        type = types.int;
        default = 30;
        description = "Seconds before marking a node as down";
      };
    };

    client = {
      enable = mkEnableOption "tatara client";

      serverAddr = mkOption {
        type = types.str;
        default = "127.0.0.1:4647";
        description = "Server gRPC address to connect to";
      };

      logLevel = mkOption {
        type = types.str;
        default = "info";
        description = "Log level";
      };

      allocDir = mkOption {
        type = types.str;
        default = "~/.local/share/tatara/alloc";
        description = "Directory for allocation data";
      };
    };
  };

  config = let
    serverConfig = lib.generators.toTOML {} {
      server = {
        http_addr = cfg.server.httpAddr;
        grpc_addr = cfg.server.grpcAddr;
        log_level = cfg.server.logLevel;
        state = { dir = cfg.server.stateDir; };
        scheduler = {
          eval_interval_secs = cfg.server.evalIntervalSecs;
          heartbeat_grace_secs = cfg.server.heartbeatGraceSecs;
        };
      };
    };

    clientConfig = lib.generators.toTOML {} {
      client = {
        server_addr = cfg.client.serverAddr;
        log_level = cfg.client.logLevel;
        alloc_dir = cfg.client.allocDir;
      };
    };
  in mkMerge [
    (mkIf cfg.server.enable (mkMerge [
      {
        xdg.configFile."tatara/server.toml".text = serverConfig;
      }
      (mkIf isDarwin (hmHelpers.mkLaunchdService {
        name = "tatara-server";
        label = "io.pleme.tatara.server";
        command = "tatara";
        args = [ "server" "--config" "${config.xdg.configHome}/tatara/server.toml" ];
        logDir = "${config.xdg.dataHome}/tatara/logs";
      }))
      (mkIf (!isDarwin) (hmHelpers.mkSystemdService {
        name = "tatara-server";
        description = "Tatara workload orchestrator server";
        command = "tatara";
        args = [ "server" "--config" "${config.xdg.configHome}/tatara/server.toml" ];
      }))
    ]))

    (mkIf cfg.client.enable (mkMerge [
      {
        xdg.configFile."tatara/client.toml".text = clientConfig;
      }
      (mkIf isDarwin (hmHelpers.mkLaunchdService {
        name = "tatara-client";
        label = "io.pleme.tatara.client";
        command = "tatara";
        args = [ "client" "--config" "${config.xdg.configHome}/tatara/client.toml" ];
        logDir = "${config.xdg.dataHome}/tatara/logs";
      }))
      (mkIf (!isDarwin) (hmHelpers.mkSystemdService {
        name = "tatara-client";
        description = "Tatara workload orchestrator client";
        command = "tatara";
        args = [ "client" "--config" "${config.xdg.configHome}/tatara/client.toml" ];
      }))
    ]))
  ];
}
