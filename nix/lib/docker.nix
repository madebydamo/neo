# Docker network helpers: one network per service, links between services.
#
# Every built-in service puts its containers on `serviceNetwork "<service>"`
# (`app-<service>`), so containers of different services cannot reach each
# other. SWAG joins the network of every service it proxies. `internal` stays
# a shared network for plugins that use it (everything on it reaches
# everything else on it, SWAG included). Networks are created from the
# containers' `networks` by `neo-docker-network.service`
# (nix/modules/core/docker-network.nix). Design: docs/design/network-isolation.md.
{lib, ...}: {
  libExtensions.docker = {
    neo.docker = rec {
      # Shared network of old configs and plugins. Never removed.
      sharedNetwork = "internal";

      # The network of one service. Plugins pass `plugin.name "<service>"`.
      serviceNetwork = name: "app-${name}";

      # Network of a link (see mkLink).
      linkNetwork = name: "link-${name}";

      # The one way from a container to services on the host itself (neo web,
      # Hermes, Vikunja, …): a network with a fixed bridge name and subnet.
      # The firewall accepts host traffic from this bridge only, so other
      # containers cannot reach host ports. Members (SWAG, Grafana for
      # alerts) reach the host as host.docker.internal = hostGateway.
      hostNetwork = "neo-host";
      hostSubnet = "172.31.255.0/24";
      hostGateway = "172.31.255.1";

      # Module fragment: `container` may reach services on the host as
      # host.docker.internal. Only for containers that must; everything on
      # neo-host can also reach the other members. Merge with lib.mkMerge.
      mkHostAccess = container: {
        virtualisation.oci-containers.containers.${container} = {
          networks = lib.mkAfter [hostNetwork];
          extraOptions = ["--add-host=host.docker.internal:${hostGateway}"];
        };
      };

      # Name of one `networks` entry: "app-x" or "name=app-x,alias=y" → "app-x".
      networkName = entry: let
        m = builtins.match "name=([^,]+).*" entry;
      in
        if m == null
        then entry
        else builtins.head m;

      # Entries that are a Docker network to create (not host, none, the
      # default bridge or another container's namespace).
      isCreatedNetwork = name:
        !(lib.elem name ["host" "none" "bridge" "default"])
        && !(lib.hasPrefix "container:" name);

      # Network names a container joins through `networks` and
      # `--network=…` / `--net=…` in extraOptions.
      containerNetworks = c: let
        fromOptions =
          lib.concatMap (
            o: let
              m = builtins.match "--net(work)?=(.+)" o;
            in
              lib.optional (m != null) (builtins.elemAt m 1)
          )
          (c.extraOptions or []);
      in
        lib.filter isCreatedNetwork (map networkName ((c.networks or []) ++ fromOptions));

      # Network names used by any of `containers` (an oci-containers attrset).
      usedNetworks = containers:
        lib.unique (lib.concatMap containerNetworks (lib.attrValues containers));

      # Module fragment: network `link-<name>` joined by exactly `containers`
      # (container names, core or plugin). For an endpoint only reachable
      # inside Docker (e.g. Collabora's WOPI for Nextcloud); prefer the
      # public URL when the app supports it. Merge with lib.mkMerge.
      mkLink = {
        name,
        containers,
      }: {
        virtualisation.oci-containers.containers = lib.genAttrs containers (_: {
          # After the service network: the first network is the one tools read
          # (monitoring, docker inspect).
          networks = lib.mkAfter [(linkNetwork name)];
        });
        neo.core.docker.links.${name} = containers;
      };

      # { <container> = <service>; } for enabled services: keys of
      # `containers` and `docker-*` entries of `systemdUnits`.
      containerService = config:
        lib.foldl' (acc: x: acc // x) {} (
          lib.mapAttrsToList (
            sname: svc: let
              fromImages = lib.attrNames (svc.containers or {});
              fromUnits =
                map (lib.removePrefix "docker-")
                (lib.filter (lib.hasPrefix "docker-") (svc.systemdUnits or []));
            in
              lib.genAttrs (fromImages ++ fromUnits) (_: sname)
          ) (lib.filterAttrs (_: svc: svc.enabled or false) (config.neo.services or {}))
        );
    };
  };
}
