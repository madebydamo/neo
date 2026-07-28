# Rathole client service options.
{...}: {
  flake.modules.nixos.rathole-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.rathole = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "rathole client service" {rank = 0;};
              token = mkOption {
                type = types.str;
                description = "Authentication token for rathole";
                rank = 10;
                helper = lib.neo.helpers.randomToken;
              };
              remoteAddr = mkOption {
                type = types.str;
                description = "Remote server address for rathole";
                rank = 20;
              };
              port = mkOption {
                type = types.port;
                default = 2333;
                description = "Remote server port for rathole";
                rank = 30;
              };
              name = mkOption {
                type = types.str;
                description = "Name prefix for rathole services";
                rank = 40;
              };
            }
            // lib.neo.mkSystemdUnits [
              "rathole"
            ]
            // lib.neo.mkServiceMeta {
              category = "Network";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/png/rathole.png";
              description = ''
                Rathole is a lightweight, high-performance reverse proxy for NAT traversal, written in Rust as a fast and low-resource alternative to frp and ngrok.
                In this homeserver, it runs as a client that securely tunnels local HTTP (port 80) and HTTPS (SWAG PROXY-protocol port) through a remote server with a public IP, using per-service tokens for auth.
                This allows exposing internal services to the internet without requiring inbound firewall rules or port forwards on the client network.
                Features include TCP/UDP support, optional encryption via TLS or Noise protocol, hot configuration reload, and minimal memory footprint (binary can be ~500KiB).
              '';
              githubUrl = "https://github.com/rathole-org/rathole";
              releaseUrl = "https://github.com/rathole-org/rathole/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Rathole client configuration";
      };
    };
}
