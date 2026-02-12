{
  config,
  lib,
  ...
}:
with lib; {
  options.neo.services.openclaw = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (lib.mdDoc "OpenClaw service");
        gatewayPort = mkOption {
          type = types.port;
          default = 18789;
          description = lib.mdDoc "Port for the OpenClaw gateway";
        };
        bridgePort = mkOption {
          type = types.port;
          default = 18790;
          description = lib.mdDoc "Port for the OpenClaw bridge";
        };
        gatewayBind = mkOption {
          type = types.str;
          default = "lan";
          description = lib.mdDoc "Gateway bind mode";
        };
        gatewayToken = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Gateway authentication token (auto-generated if null)";
        };
        image = mkOption {
          type = types.str;
          default = "moltbot/moltbot:latest";
          description = lib.mdDoc "Docker image for OpenClaw";
        };
        claudeAiSessionKey = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Claude AI session key";
        };
        claudeWebSessionKey = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Claude web session key";
        };
        claudeWebCookie = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Claude web cookie";
        };
        additionalMountPoints = mkOption {
          type = types.attrsOf (types.listOf types.str);
          default = {};
          description = lib.mdDoc "Additional volume mounts";
        };
        subdomain = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Subdomain for the service";
        };
        proxyConf = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = lib.mdDoc "Nginx proxy conf for swag";
        };
      };
    };
    default = {};
    description = lib.mdDoc "OpenClaw service configuration";
  };
}
