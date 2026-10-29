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
              token = lib.neo.mkSecretOption {
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
                default = 2223;
                description = "Remote server port for rathole (the Neo streamproxy tunnel port is 2223)";
                rank = 30;
              };
              name = mkOption {
                type = types.str;
                description = "Name prefix for rathole services";
                rank = 40;
              };
              certificateOnly = mkOption {
                type = types.bool;
                default = false;
                description = ''
                  Tunnel HTTP only (Let's Encrypt HTTP-01). Public HTTPS is not exposed; services stay reachable on LAN or Tailscale.
                  Requires Pi-hole with localIP, or Tailscale split DNS, so hostnames still resolve privately after public HTTPS is dropped.
                '';
                rank = 50;
              };
              extraServices = mkOption {
                type = types.attrsOf (types.submodule {
                  options = {
                    localAddr = mkOption {
                      type = types.str;
                      description = "Client-side local_addr (e.g. 127.0.0.1:2222 for Gitea SSH)";
                    };
                  };
                });
                default = {};
                rank = 60;
                description = "Additional rathole client TCP services beyond _http/_https (name → localAddr)";
              };
            }
            // lib.neo.mkSystemdUnits [
              "rathole"
            ]
            // lib.neo.mkServiceMeta {
              category = "Network";
              icon = ./icon.png;
              tagline = "Makes your apps reachable from the internet without opening your router";
              description = ''
                Connects your homeserver to a small rented server with a public address (a VPS), so your apps can be reached from anywhere.
                Works even when your internet provider does not allow incoming connections.
              '';
              notes = ''
                Rathole client: tunnels HTTP (port 80) and HTTPS (SWAG's PROXY-protocol port) to a rathole server, authenticated with per-service tokens. No port forwards or inbound firewall rules needed.
                Pair it with a streamproxy server. Supports TCP/UDP and optional TLS or Noise encryption.
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
