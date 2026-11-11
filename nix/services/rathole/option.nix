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
              enabled = mkEnableOption "a tunnel through your own public server, so your apps are reachable from the internet without opening ports at home" {rank = 0;};
              # Same token for every [client.services.*] entry.
              token = lib.neo.mkSecretOption {
                description = "Shared password between this homeserver and the tunnel server. Use the same value on both.";
                rank = 10;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Tunnel password";};
              };
              # rathole [client] remote_addr host part.
              remoteAddr = mkOption {
                type = types.str;
                description = "Address of your public tunnel server (VPS) that passes internet traffic to your homeserver.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Tunnel server address";};
              };
              # The Neo streamproxy tunnel port is 2223.
              port = mkOption {
                type = types.port;
                default = 2223;
                description = "Port of the tunnel server. Leave at 2223 unless your tunnel server uses another one.";
                rank = 30;
                ui = lib.neo.ui.mkUi {
                  label = "Tunnel server port";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              # Prefix of the rathole service names (<name>_http, <name>_https, <name>_<extra>).
              name = mkOption {
                type = types.str;
                description = "Name of this homeserver on the tunnel server. Must match the tunnel server's entry.";
                rank = 40;
                ui = lib.neo.ui.mkUi {label = "Homeserver name";};
              };
              # Tunnels only HTTP (Let's Encrypt HTTP-01); the _https service is dropped.
              # Asserted: needs Pi-hole with localIP, Tailscale split DNS or NetBird app
              # names, so hostnames still resolve privately.
              certificateOnly = mkOption {
                type = types.bool;
                default = false;
                description = "Use the tunnel only to get HTTPS certificates; your apps stay reachable only at home or over Tailscale or NetBird. Needs Pi-hole with Home network address set, App names over Tailscale, or App names over NetBird.";
                rank = 50;
                ui = lib.neo.ui.mkUi {label = "Certificates only (keep apps private)";};
              };
              # Extra [client.services.<name>_<key>] entries beyond _http/_https.
              extraServices = mkOption {
                type = types.attrsOf (types.submodule {
                  options = {
                    # rathole local_addr.
                    localAddr = mkOption {
                      type = types.str;
                      description = "Address and port on this homeserver to pass through (e.g. 127.0.0.1:2222 for Gitea SSH).";
                      ui = lib.neo.ui.mkUi {label = "Local address";};
                    };
                  };
                });
                default = {};
                rank = 60;
                description = "Extra connections to pass through the tunnel besides web traffic, by name. Each name needs a matching entry on the tunnel server.";
                ui = lib.neo.ui.mkUi {
                  label = "Extra tunnels";
                  group = lib.neo.ui.groups.advanced;
                };
              };
            }
            // lib.neo.mkSystemdUnits [
              "rathole"
            ]
            // lib.neo.mkServiceMeta {
              category = "Network";
              license = {
                spdx = "Apache-2.0";
                url = "https://github.com/rathole-org/rathole/blob/main/LICENSE";
              };
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
