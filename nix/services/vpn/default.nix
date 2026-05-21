# VPN (gluetun) service implementation with automatic overlay for VPN-routed services.
{...}: {
  flake.modules.nixos.docker-networks = {
    config,
    lib,
    ...
  }:
    with lib; let
      services = neo.getDockerNetworkServices config;

      patch = value: {networks = value.vpn.networks;};
      all-containers = value: value.vpn.internalContainers ++ value.vpn.containers;
      mapper = _: value: map (containerName: {${containerName} = patch value;}) (all-containers value);

      patchedContainers = mergeAttrsList (flatten (attrsets.mapAttrsToList mapper services));
    in {
      config = {
        virtualisation.oci-containers.containers = patchedContainers;
      };
    };
  flake.modules.nixos.vpn = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.vpn;
      services = neo.getVpnServices config;

      patch = {
        networks = mkForce ["container:vpn"];
        dependsOn = ["vpn"];
      };

      routedContainerNames = flatten (map (service: service.vpn.containers) (attrValues services));
      vpnAliases = map (name: "--network-alias=${name}") routedContainerNames;
      routedNetworks = lists.unique (flatten (mapAttrsToList (name: value: value.vpn.networks) services));
      patchedContainers = listToAttrs (map (name: nameValuePair name patch) routedContainerNames);
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfg.wireguardPrivateKey != null && cfg.wireguardPresharedKey != null && cfg.wireguardAddresses != null;
            message = "You must configure your wireguard tunnel with a private key, preshared key and an address";
          }
        ];

        virtualisation.oci-containers.containers =
          {
            vpn = {
              image = cfg.image;
              autoStart = true;
              capabilities = {
                NET_ADMIN = true;
              };
              devices = [
                "/dev/net/tun:/dev/net/tun"
              ];
              environment =
                {
                  VPN_SERVICE_PROVIDER = cfg.vpnServiceProvider;
                  VPN_TYPE = "wireguard";
                  WIREGUARD_PRIVATE_KEY = cfg.wireguardPrivateKey;
                  WIREGUARD_PRESHARED_KEY = cfg.wireguardPresharedKey;
                  WIREGUARD_ADDRESSES = cfg.wireguardAddresses;
                  SERVER_COUNTRIES = cfg.serverCountries;
                }
                // optionalAttrs (cfg.firewallVpnInputPorts != null) {
                  FIREWALL_VPN_INPUT_PORTS = cfg.firewallVpnInputPorts;
                };
              networks = routedNetworks;
              extraOptions = vpnAliases;
            };
          }
          // patchedContainers;
      };
    };
}
