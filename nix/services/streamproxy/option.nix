{...}: {
  flake.modules.nixos.streamproxy-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      # The submodule below shadows `config`.
      swagEnabled = config.neo.services.swag.enabled or false;
    in {
      options.neo.services.streamproxy = mkOption {
        type = types.submodule (
          {config, ...}: let
            names = attrNames config.entries;
            sortedNames = sort (a: b: a < b) names;
            computedPorts = listToAttrs (
              imap0 (i: name: {
                name = name;
                value = {
                  https = 10000 + i * 2;
                  http = 10001 + i * 2;
                };
              })
              sortedNames
            );
          in {
            options =
              {
                enabled = mkEnableOption "sharing this server's public IP address with other homeservers through tunnels" {rank = 0;};
                entries = mkOption {
                  type = types.attrsOf (
                    types.submodule {
                      options = {
                        url = mkOption {
                          type = types.str;
                          description = "Domain whose traffic goes to this tunnel, e.g. home.example.com.";
                          ui = lib.neo.ui.mkUi {label = "Domain";};
                          rank = 0;
                        };
                        token = lib.neo.mkSecretOption {
                          type = types.str;
                          # rathole server token for this entry
                          description = "Shared password the remote homeserver uses for its tunnel. Use the same value on both sides.";
                          ui = lib.neo.ui.mkUi {label = "Tunnel password";};
                          helper = lib.neo.helpers.randomToken;
                          rank = 10;
                        };
                        wildcard = mkOption {
                          type = types.bool;
                          default = false;
                          description = "Also send every subdomain of the domain (e.g. photos.home.example.com) through this tunnel.";
                          ui = lib.neo.ui.mkUi {label = "Include subdomains";};
                          rank = 20;
                        };
                        includeTopLevel = mkOption {
                          type = types.bool;
                          default = true;
                          description = "Send the domain itself (without a subdomain) through this tunnel.";
                          ui = lib.neo.ui.mkUi {label = "Include the domain itself";};
                          rank = 30;
                        };
                        customDomains = mkOption {
                          type = types.listOf types.str;
                          default = [];
                          description = "More domains whose traffic goes to this tunnel, one per line.";
                          ui = lib.neo.ui.mkUi {label = "Extra domains";};
                          rank = 40;
                        };
                      };
                    }
                  );
                  default = {};
                  rank = 10;
                  description = "One tunnel per remote homeserver. Each tunnel gets the traffic for its domains.";
                  ui = lib.neo.ui.mkUi {label = "Tunnels";};
                };
                tcpForwards = mkOption {
                  type = types.attrsOf (
                    types.submodule {
                      options = {
                        entry = mkOption {
                          type = types.str;
                          description = "Name of the tunnel (from Tunnels) that receives this port.";
                          ui = lib.neo.ui.mkUi {label = "Tunnel";};
                          rank = 0;
                        };
                        serviceSuffix = mkOption {
                          type = types.str;
                          # rathole service name is <entry>_<serviceSuffix>; the client must match.
                          description = "Short name for this port, e.g. gitea_ssh. The remote homeserver must use the same name.";
                          ui = lib.neo.ui.mkUi {label = "Port name";};
                          example = "gitea_ssh";
                          rank = 10;
                        };
                        listenPort = mkOption {
                          type = types.port;
                          description = "Public port number to open, e.g. 2222 for Git over SSH.";
                          ui = lib.neo.ui.mkUi {label = "Public port";};
                          rank = 20;
                        };
                      };
                    }
                  );
                  default = {};
                  rank = 20;
                  # Raw TCP via rathole (not SNI/HTTPS); each forward adds a rathole server
                  # service bound on listenPort and opens the firewall.
                  description = "Open extra public ports (e.g. Git over SSH) and send them through a tunnel.";
                  ui = lib.neo.ui.mkUi {
                    label = "Extra public ports";
                    group = lib.neo.ui.groups.advanced;
                  };
                };
                ports = mkOption {
                  type = types.attrsOf (types.attrsOf types.int);
                  internal = true;
                  readOnly = true;
                  default = computedPorts;
                  description = "Automatically assigned ports for each streamproxy entry";
                };
              }
              // lib.neo.mkSystemdUnits ([
                  "streamproxy-local80"
                  "streamproxy-local443"
                  "container@streamproxy"
                ]
                # Only exists next to a local SWAG.
                ++ lib.optional swagEnabled "streamproxy-swag-pp")
              // lib.neo.mkServiceMeta {
                category = "Core";
                license = {
                  spdx = "MIT";
                  url = "https://github.com/madebydamo/neo/blob/master/LICENSE";
                };
                icon = ./icon.svg;
                tagline = "Lets several homeservers share one public address";
                description = ''
                  Runs on a rented server with a public address and passes internet traffic on to one or more homeservers through secure tunnels.
                  Only needed if you run the public side of rathole yourself.
                '';
                notes = ''
                  Runs nginx (HTTP routing and SNI stream proxy for HTTPS) and a rathole server inside a NixOS container.
                  Accepts rathole tunnels from remote homeservers (port 2223) and routes each configured domain (wildcards, top-level and extra domains) to the local SWAG or to that entry's tunnel port.
                  Tunnel ports (from 10000) and nginx blocks are generated from the entries.
                '';
                githubUrl = "https://github.com/madebydamo/neo";
              }
              // lib.neo.mkSkillOptions {};
          }
        );
        default = {};
        description = "Streamproxy service configuration (host nginx for 80/443 + rathole server)";
      };
    };
}
