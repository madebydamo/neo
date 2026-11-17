# VPN (gluetun) service implementation with automatic overlay for VPN-routed services.
{...}: {
  flake.modules.nixos.docker-networks = {
    config,
    lib,
    ...
  }:
    with lib; let
      services = neo.getDockerNetworkServices config;

      # mkDefault: a container that names its own networks keeps them. Adding
      # vpn.networks on top would list a network twice, which Docker refuses
      # ("network … is specified multiple times").
      patch = value: {networks = mkDefault value.vpn.networks;};
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
      inherit (lib.neo) docker;
      dockerCfg = config.neo.core.docker;

      # Routed containers live in gluetun's namespace, so gluetun stands in
      # for them on every network they would be on, answering for their names:
      #   - the networks their service lists in vpn.networks, and the
      #     service's own network (SWAG reaches the app there);
      #   - every link (mkLink) a routed container is a member of, so the
      #     other members reach it by name and it reaches them;
      #   - neo-host when a routed container has mkHostAccess.
      # Aliases per network: `--network-alias` would apply to the first
      # network only, and each name must resolve only on its own networks
      # (app-firefox must not resolve searxng). So a link or another
      # service's network sees just the names that belong there.
      memberships =
        flatten (mapAttrsToList (sname: svc:
          map (net: {
            inherit net;
            aliases = svc.vpn.containers;
          }) (svc.vpn.networks ++ optional (svc.vpn.containers != []) (docker.serviceNetwork sname)))
        services)
        ++ flatten (mapAttrsToList (link: members: let
          routedMembers = filter (c: elem c routedContainerNames) members;
        in
          optional (routedMembers != []) {
            net = docker.linkNetwork link;
            aliases = routedMembers;
          })
        dockerCfg.links);
      hostAccess = any (c: elem c routedContainerNames) dockerCfg.hostAccess;
      routedNetworks = unique (map (m: m.net) memberships);
      aliasesOn = net: unique (concatMap (m: optionals (m.net == net) m.aliases) memberships);
      gluetunNetworks =
        map (
          net: let
            aliases = aliasesOn net;
          in
            if aliases == []
            then net
            else concatStringsSep "," (["name=${net}"] ++ map (a: "alias=${a}") aliases)
        )
        routedNetworks
        ++ optional hostAccess docker.hostNetwork;
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
        assertions =
          lib.neo.requireSecrets "vpn" cfg [
            {
              name = "wireguardPrivateKey";
              hint = "the PrivateKey line of your provider's WireGuard config";
            }
            {
              name = "wireguardAddresses";
              hint = "the Address line of your provider's WireGuard config";
            }
          ]
          ++ [
            {
              # Empty is allowed: some providers have no preshared key.
              assertion = cfg.wireguardPresharedKey != null;
              message = "neo.services.vpn: wireguardPresharedKey must be set when enabled (the PresharedKey line of your provider's WireGuard config; empty when it has none).";
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

        # WireGuard keys reach gluetun through an env file rendered at activation.
        neo.core.secrets.containerEnv.vpn = {
          WIREGUARD_PRIVATE_KEY = cfg.wireguardPrivateKey;
          WIREGUARD_PRESHARED_KEY = cfg.wireguardPresharedKey;
        };

        virtualisation.oci-containers.containers =
          {
            vpn = {
              image = cfg.image;
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
                  WIREGUARD_ADDRESSES = cfg.wireguardAddresses;
                  SERVER_COUNTRIES = cfg.serverCountries;
                }
                // optionalAttrs (cfg.firewallVpnInputPorts != null) {
                  FIREWALL_VPN_INPUT_PORTS = cfg.firewallVpnInputPorts;
                };
              networks = gluetunNetworks;
              extraOptions = optional hostAccess docker.hostAlias;
            };
          }
          // patchedContainers;
      };
    };
}
