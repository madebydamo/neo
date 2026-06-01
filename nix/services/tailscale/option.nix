# Tailscale service options.
{...}: {
  flake.modules.nixos.tailscale-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.tailscale = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Tailscale service";
              authKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Node authorization key; if provided and device not logged in, will be used for authentication";
              };
              acceptDns = mkOption {
                type = types.bool;
                default = true;
                description = "Accept DNS configuration from the admin panel";
              };
              acceptRoutes = mkOption {
                type = types.bool;
                default = false;
                description = "Accept routes advertised by other Tailscale nodes";
              };
              advertiseExitNode = mkOption {
                type = types.bool;
                default = false;
                description = "Offer to be an exit node for internet traffic for the tailnet";
              };
              advertiseRoutes = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Routes to advertise to other nodes (list of CIDR strings, e.g. [\"10.0.0.0/8\", \"192.168.0.0/24\"])";
              };
              exitNode = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Tailscale exit node (IP or base name) for internet traffic";
              };
              exitNodeAllowLanAccess = mkOption {
                type = types.bool;
                default = false;
                description = "Allow direct access to the local network when routing traffic via an exit node";
              };
              hostname = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Hostname to use instead of the one provided by the OS";
              };
              loginServer = mkOption {
                type = types.str;
                default = "https://controlplane.tailscale.com";
                description = "Base URL of control server";
              };
              ssh = mkOption {
                type = types.bool;
                default = false;
                description = "Run an SSH server, permitting access per tailnet admin's declared policy";
              };
            }
            // lib.neo.mkServiceMeta {
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/tailscale.svg";
              description = ''
                Tailscale is the easiest, most secure way to use WireGuard for creating a mesh VPN (tailnet) that connects devices and services across networks with zero-config NAT traversal and identity-based access controls.
                In this homeserver, it provides simple remote access to the server and resources from anywhere, with support for exit nodes, advertised routes, MagicDNS, and SSH gated by tailnet policy — all without manual port forwarding or complex firewall rules.
                Built on fully open source client code (tailscaled daemon + CLI), it offers SSO, ACLs, stable 100.x IPs, and works seamlessly for homelab, multi-cloud, and IoT scenarios.
              '';
              projectUrl = "https://tailscale.com/";
              githubUrl = "https://github.com/tailscale/tailscale";
              releaseUrl = "https://github.com/tailscale/tailscale/releases";
            };
        };
        default = {};
        description = "Tailscale service configuration";
      };
    };
}
