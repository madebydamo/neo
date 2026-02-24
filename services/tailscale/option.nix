{
  config,
  lib,
  ...
}:
with lib; {
  options.neo.services.tailscale = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (mdDoc "Tailscale service");
        authKey = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = mdDoc "Node authorization key; if provided and device not logged in, will be used for authentication";
        };
        acceptDns = mkOption {
          type = types.bool;
          default = true;
          description = mdDoc "Accept DNS configuration from the admin panel";
        };
        acceptRoutes = mkOption {
          type = types.bool;
          default = false;
          description = mdDoc "Accept routes advertised by other Tailscale nodes";
        };
        advertiseExitNode = mkOption {
          type = types.bool;
          default = false;
          description = mdDoc "Offer to be an exit node for internet traffic for the tailnet";
        };
        advertiseRoutes = mkOption {
          type = types.listOf types.str;
          default = [];
          description = mdDoc "Routes to advertise to other nodes (list of CIDR strings, e.g. [\"10.0.0.0/8\", \"192.168.0.0/24\"])";
        };
        exitNode = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = mdDoc "Tailscale exit node (IP or base name) for internet traffic";
        };
        exitNodeAllowLanAccess = mkOption {
          type = types.bool;
          default = false;
          description = mdDoc "Allow direct access to the local network when routing traffic via an exit node";
        };
        hostname = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = mdDoc "Hostname to use instead of the one provided by the OS";
        };
        loginServer = mkOption {
          type = types.str;
          default = "https://controlplane.tailscale.com";
          description = mdDoc "Base URL of control server";
        };
        ssh = mkOption {
          type = types.bool;
          default = false;
          description = mdDoc "Run an SSH server, permitting access per tailnet admin's declared policy";
        };
      };
    };
    default = {};
    description = mdDoc "Tailscale service configuration";
  };
}
