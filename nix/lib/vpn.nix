# VPN helper types and getters for automatic VPN overlay.
{lib, ...}: {
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
          vpn = mkOption {
            type = types.submodule {
              options = {
                enabled = mkOption {
                  type = types.bool;
                  default = enabled;
                  description = "Put selected outbound containers behind the VPN (gluetun)";
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
                  description = "Host ports exposed by the VPN-routed containers (for firewall rules/validation)";
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
            description = "VPN routing options for this service";
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
