{...}: {
  flake.modules.nixos.streamproxy-option = {
    config,
    lib,
    ...
  }:
    with lib; {
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
            options = {
              enabled = mkEnableOption ("streamproxy nginx + rathole server for public IP sharing");
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
            };
          }
        );
        default = {};
        description = "Streamproxy service configuration (host nginx for 80/443 + rathole server)";
      };
    };
}
