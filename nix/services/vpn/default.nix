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

      # Each VPN-routed container joins gluetun's network namespace, so listen ports must be unique.
      portClaims = flatten (
        mapAttrsToList (
          sname: svc:
            map (port: {
              service = sname;
              inherit port;
            }) (svc.vpn.ports or [])
        )
        services
      );
      claimedPorts = map (c: c.port) portClaims;
      duplicatePorts = unique (filter (p: (count (c: c.port == p) portClaims) > 1) claimedPorts);
      portConflictMessage =
        concatMapStringsSep "\n" (
          p: let
            owners = filter (c: c.port == p) portClaims;
            names = concatMapStringsSep ", " (c: c.service) owners;
          in "  - port ${toString p}: claimed by ${names}"
        )
        duplicatePorts;
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfg.wireguardPrivateKey != null && cfg.wireguardPresharedKey != null && cfg.wireguardAddresses != null;
            message = "You must configure your wireguard tunnel with a private key, preshared key and an address";
          }
          {
            assertion = duplicatePorts == [];
            message = ''
              neo.services.vpn: overlapping container ports among VPN-routed services.
              Containers with vpn.enabled share the gluetun network namespace, so only one process can bind each port.
              Resolve by rebinding a service port (and its vpn.ports list) so claims are unique:
              ${portConflictMessage}
            '';
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
