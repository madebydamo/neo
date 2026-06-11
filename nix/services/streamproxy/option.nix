{...}: {
  flake.modules.nixos.streamproxy-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
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
                enabled = mkEnableOption "streamproxy nginx + rathole server for public IP sharing" {rank = 0;};
                entries = mkOption {
                  type = types.attrsOf (
                    types.submodule {
                      options = {
                        url = mkOption {
                          type = types.str;
                          description = "The domain name for this ingress entry";
                        };
                        token = mkOption {
                          type = types.str;
                          description = "The authentication token for rathole";
                        };
                        wildcard = mkOption {
                          type = types.bool;
                          default = false;
                          description = "Whether to route subdomains (*.url) to this ingress";
                        };
                        includeTopLevel = mkOption {
                          type = types.bool;
                          default = true;
                          description = "Whether to route the top-level domain (url) to this ingress";
                        };
                        customDomains = mkOption {
                          type = types.listOf types.str;
                          default = [];
                          description = "Additional custom domains that should be routed to this entry (certificates + forwarding)";
                        };
                      };
                    }
                  );
                  default = {};
                  description = "Streamproxy entries mapping names to URLs, tokens, and routing rules";
                };
                ports = mkOption {
                  type = types.attrsOf (types.attrsOf types.int);
                  internal = true;
                  readOnly = true;
                  default = computedPorts;
                  description = "Automatically assigned ports for each streamproxy entry";
                };
              }
              // lib.neo.mkServiceMeta {
                icon = "🔀";
                description = ''
                  Streamproxy is a custom Neo homeserver component that runs Nginx (HTTP routing + SNI stream proxy for HTTPS) together with a Rathole server inside a NixOS container.
                  It enables multiple independent homeserver instances to share a single public IP by accepting secure rathole tunnels from remote clients (on port 2223) and routing traffic for configured domains (with support for wildcards, top-level, and extra custom domains) either to the local SWAG or to the per-entry tunnel ports via socat forwarding.
                  The computed dynamic ports (starting at 10000) and nginx config blocks are generated automatically from the entries; this provides centralized ingress without each homeserver needing dedicated public IPs, open ports, or complex DNS/HAProxy setups.
                '';
                githubUrl = "https://github.com/madebydamo/neo";
              };
          }
        );
        default = {};
        description = "Streamproxy service configuration (host nginx for 80/443 + rathole server)";
      };
    };
}
