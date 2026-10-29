# VPN helper types and getters for automatic VPN overlay.
# Rank 110 places the vpn.* group after subdomain (100) and before auth (120).
{lib, ...}: let
  neoUi = (import ./ui.nix {inherit lib;}).libExtensions.ui.neo.ui;
in {
  libExtensions.vpn = {
    neo = {
      mkVpnOptions = {
        containers ? [],
        internalContainers ? [],
        ports ? [],
        networks ? [],
        enabled ? false,
      }:
        with lib; {
          vpn =
            mkOption {
              type = types.submodule {
                options = {
                  enabled =
                    mkOption {
                      type = types.bool;
                      default = enabled;
                      # Routes vpn.containers through the gluetun network namespace.
                      description = "Send this app's internet traffic through your VPN service. Needs the VPN app to be turned on.";
                    }
                    // {
                      rank = 0;
                      ui = neoUi.mkUi {
                        label = "Route through VPN";
                        summary = true;
                      };
                    };
                  containers = mkOption {
                    type = types.listOf types.str;
                    internal = true;
                    default = containers;
                    description = "Container names (within this service) that should use the VPN network";
                  };
                  internalContainers = mkOption {
                    type = types.listOf types.str;
                    internal = true;
                    default = internalContainers;
                    description = "Container names that should not use the VPN network";
                  };
                  ports = mkOption {
                    type = types.listOf types.port;
                    internal = true;
                    default = ports;
                    description = ''
                      Listen ports of VPN-routed containers in this service.
                      Used for conflict detection: all containers with vpn.enabled share gluetun's network
                      namespace, so these ports must be unique across every VPN-routed service.
                    '';
                  };
                  networks = mkOption {
                    type = types.listOf types.str;
                    internal = true;
                    default = networks;
                    description = "Networks the outbound containers are in";
                  };
                };
              };
              default = {
                inherit enabled containers internalContainers ports networks;
              };
              description = "Whether this app's internet traffic goes through your VPN.";
            }
            // {
              rank = 110;
              ui = neoUi.mkUi {group = neoUi.groups.access;};
            };
        };

      # Returns attrset of services (name -> cfg) that have vpn.enabled = true
      getVpnServices = config:
        lib.filterAttrs (
          name: value:
            (value.enabled or false)
            && (value.vpn.enabled or false)
        )
        config.neo.services;
      getDockerNetworkServices = config:
        lib.filterAttrs (
          name: value:
            (value.enabled or false)
            && (value.vpn.networks or [] != [])
        )
        config.neo.services;
    };
  };
}
