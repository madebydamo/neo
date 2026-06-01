# Rathole client service options.
{...}: {
  flake.modules.nixos.rathole-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.rathole = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "rathole client service";
              token = mkOption {
                type = types.str;
                description = "Authentication token for rathole";
              };
              remoteAddr = mkOption {
                type = types.str;
                description = "Remote server address for rathole";
              };
              port = mkOption {
                type = types.port;
                default = 2333;
                description = "Remote server port for rathole";
              };
              name = mkOption {
                type = types.str;
                description = "Name prefix for rathole services";
              };
            }
            // lib.neo.mkServiceMeta {
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/png/rathole.png";
              description = ''
                Rathole is a lightweight, high-performance reverse proxy for NAT traversal, written in Rust as a fast and low-resource alternative to frp and ngrok.
                In this homeserver, it runs as a client that securely tunnels local HTTP and HTTPS traffic (on ports 80 and 443) through a remote server with a public IP, using per-service tokens for auth.
                This allows exposing internal services to the internet without requiring inbound firewall rules or port forwards on the client network.
                Features include TCP/UDP support, optional encryption via TLS or Noise protocol, hot configuration reload, and minimal memory footprint (binary can be ~500KiB).
              '';
              githubUrl = "https://github.com/rathole-org/rathole";
              releaseUrl = "https://github.com/rathole-org/rathole/releases";
            };
        };
        default = {};
        description = "Rathole client configuration";
      };
    };
}
