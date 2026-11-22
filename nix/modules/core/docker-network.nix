# Docker networks for the apps (docs/design/network-isolation.md).
#
# `app-<service>`: one per service (`lib.neo.docker.serviceNetwork`). Only that
# service's containers, SWAG (when the service has a site) and gluetun (when
# it is routed through the VPN) are on it, so apps cannot reach each other.
#
# `link-<name>`: a few containers of different services that must talk inside
# Docker (`lib.neo.docker.mkLink`).
#
# `internal`: the shared network of plugins written before per-service
# networks. SWAG is on it; everything on it reaches everything else on it.
# Kept as is so those plugins keep working.
#
# `neo-host`: the only bridge from which the host accepts connections (its
# own services: neo web, Hermes, Vikunja, …). Fixed bridge name and subnet;
# SWAG and Grafana (alerts to Hermes) join it (`lib.neo.docker.mkHostAccess`).
# Any other container gets no answer from host ports.
#
# `swag-ipv6`: IPv6 only, SWAG alone. Without an IPv6 network Docker hands an
# IPv6 connection to a published port to its userland proxy, which reconnects
# over IPv4 from the gateway: SWAG saw every IPv6 visitor as 172.18.0.1, a
# "Home network" address. With an IPv6 endpoint Docker forwards the connection
# and keeps its source address. The apps stay on IPv4: most listen on IPv4 only,
# and an IPv6 address in Docker's DNS would send their clients there first.
#
# Every network a container names is created here (`neo.core.docker.networks`
# is filled from the containers), labelled neo.managed=1. Managed networks no
# one declares any more are removed once no container uses them, so a disabled
# service leaves no network behind.
{...}: {
  flake.modules.nixos.docker-network = {
    config,
    lib,
    pkgs,
    ...
  }: let
    inherit (lib.neo) docker;
    dockerBin = "${config.virtualisation.docker.package}/bin/docker";
    ipv6Net = lib.neo.swagIpv6Network;
    cfg = config.neo.core.docker;
    containers = config.virtualisation.oci-containers.containers;

    # Created by Neo before networks were labelled. Removed like managed ones
    # once no one declares them and they are empty. Never `internal`.
    # Remove after 2027-01: by then every server ran this since 2026-11-14 and dropped it.
    legacyNetworks = ["monitoring"];

    ipv4Networks = lib.filterAttrs (n: _: n != ipv6Net) cfg.networks;

    # VPN-routed containers have no networks of their own (container:vpn):
    # gluetun joins their links and neo-host for them (vpn/default.nix).
    routed = lib.neo.vpnRoutedContainers config;

    # Fixed subnets first, before the pool hands their range to another network.
    networkLines = map (name: let
      net = ipv4Networks.${name};
      field = v: toString (lib.defaultTo "" v);
    in "${name}|${lib.boolToString net.internal}|${field net.subnet}|${field net.gateway}|${field net.bridgeName}")
    (lib.sort (a: b: (ipv4Networks.${a}.subnet != null) && (ipv4Networks.${b}.subnet == null)) (lib.attrNames ipv4Networks));

    script = lib.getExe (pkgs.writeShellApplication {
      name = "neo-docker-network";
      runtimeInputs = [config.virtualisation.docker.package pkgs.coreutils pkgs.gawk pkgs.gnugrep pkgs.iproute2];
      runtimeEnv = {
        NEO_NETWORKS = lib.concatStringsSep "\n" networkLines;
        NEO_IPV6_NETWORK = ipv6Net;
        NEO_SHARED_NETWORK = docker.sharedNetwork;
        NEO_DECLARED = lib.concatStringsSep " " (lib.attrNames cfg.networks);
        NEO_LEGACY = lib.concatStringsSep "\n" legacyNetworks;
      };
      text = builtins.readFile ./docker-network.sh;
    });
  in {
    options.neo.core = {
      docker = {
        networks = lib.mkOption {
          type = lib.types.attrsOf (lib.types.submodule {
            options = {
              internal = lib.mkOption {
                type = lib.types.bool;
                default = false;
                description = "Docker --internal: containers on it get no route to the internet.";
              };
              subnet = lib.mkOption {
                type = lib.types.nullOr lib.types.str;
                default = null;
                description = "Fixed IPv4 subnet; null takes one from the address pool.";
              };
              gateway = lib.mkOption {
                type = lib.types.nullOr lib.types.str;
                default = null;
                description = "Gateway address with a fixed subnet.";
              };
              bridgeName = lib.mkOption {
                type = lib.types.nullOr (lib.types.strMatching "[a-z][a-z0-9-]{0,14}");
                default = null;
                description = "Fixed Linux bridge interface name, for firewall rules (at most 15 characters).";
              };
            };
          });
          default = {};
          internal = true;
          description = "Docker networks to create. Filled from the containers' networks; set flags here.";
        };
        links = lib.mkOption {
          type = lib.types.attrsOf (lib.types.listOf lib.types.str);
          default = {};
          internal = true;
          description = "Links between services (lib.neo.docker.mkLink): link name → containers on link-<name>.";
        };
        hostAccess = lib.mkOption {
          type = lib.types.listOf lib.types.str;
          default = [];
          internal = true;
          description = "Containers that reach host services through neo-host (lib.neo.docker.mkHostAccess).";
        };
      };
      dockerNetworks = lib.mkOption {
        type = lib.types.listOf (lib.types.strMatching "[a-z][a-z0-9-]*");
        default = [];
        internal = true;
        description = "Deprecated: extra Docker networks to create. Use neo.core.docker.networks, or just name the network in a container's networks.";
      };
    };

    config = {
      virtualisation.oci-containers.containers = lib.mkMerge [
        # Every container of a service is on the service's network unless a
        # module names its networks (any priority above this default wins:
        # explicit lists, VPN routing, vpn.networks).
        (lib.mapAttrs (_: service: {
          networks = lib.mkOverride docker.serviceNetworkPriority [(docker.serviceNetwork service)];
        }) (docker.unitContainers config))
        # A link (mkLink) or host access (mkHostAccess) adds a network at
        # normal priority, which would replace the default above: the
        # container would lose its service network and the other containers
        # of its service (issue #30, a plugin's link to immich-server). So the
        # service network comes with them, first. A module that names it too
        # is fine: oci-containers passes each network once.
        (lib.mapAttrs (_: service: {
            networks = lib.mkBefore [(docker.serviceNetwork service)];
          }) (lib.filterAttrs (c: _: !(lib.elem c routed) && lib.elem c (lib.concatLists (lib.attrValues cfg.links) ++ cfg.hostAccess))
            (docker.unitContainers config)))
        (lib.genAttrs (lib.filter (c: !(lib.elem c routed)) (lib.unique cfg.hostAccess)) (_: {
          networks = lib.mkAfter [docker.hostNetwork];
          extraOptions = [docker.hostAlias];
        }))
      ];

      neo.core.docker.networks = lib.mkMerge [
        (lib.genAttrs
          ([docker.sharedNetwork ipv6Net] ++ config.neo.core.dockerNetworks ++ docker.usedNetworks containers)
          (_: {}))
        (lib.mkIf (lib.elem docker.hostNetwork (docker.usedNetworks containers)) {
          ${docker.hostNetwork} = {
            subnet = docker.hostSubnet;
            gateway = docker.hostGateway;
            bridgeName = docker.hostNetwork;
          };
        })
      ];

      # Containers reach host ports from neo-host only (see above). Docker's
      # own rules still forward published ports and the internet.
      networking.firewall.extraCommands = ''
        iptables -I nixos-fw 1 -i ${docker.hostNetwork} -j ACCEPT
      '';
      networking.firewall.extraStopCommands = ''
        iptables -D nixos-fw -i ${docker.hostNetwork} -j ACCEPT 2>/dev/null || true
      '';

      # Docker's default pools allow about 31 bridge networks and hand out
      # 192.168.x.0/20 slices that collide with home networks. /24s of
      # 172.16.0.0/12 give 4096, in the range the NetBird check reserves for
      # Docker. Existing networks keep their subnets.
      virtualisation.docker.daemon.settings.default-address-pools = [
        {
          base = "172.16.0.0/12";
          size = 24;
        }
      ];

      # A routed member counts when gluetun (container vpn) joins the link for it.
      assertions = lib.flatten (lib.mapAttrsToList (link: members:
        map (c: let
          joins = n: containers ? ${n} && lib.elem (docker.linkNetwork link) (docker.containerNetworks containers.${n});
        in {
          assertion =
            containers
            ? ${c}
            && (
              if lib.elem c routed
              then joins "vpn"
              else joins c
            );
          message = "Docker link ${link}: container ${c} does not exist, or does not join ${docker.linkNetwork link}.";
        })
        members)
      cfg.links);

      systemd.services = lib.mkMerge [
        {
          neo-docker-network = {
            description = "Docker networks for the apps";
            after = ["docker.service"];
            requires = ["docker.service"];
            # Runs again when Docker restarts.
            partOf = ["docker.service"];
            wantedBy = ["multi-user.target"];
            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
              ExecStart = "${pkgs.util-linux}/bin/flock /run/neo-docker-network.lock ${script}";
            };
          };
        }
        # Wants, not Requires: a restart of this unit (a new script) must not
        # restart every container.
        #
        # partOf docker: the units only Require docker.service, so a restart of
        # Docker stopped every app and left it stopped. Now they restart with it.
        #
        # preStart: a container whose network is missing (Docker restarted
        # under a switch, a race with this unit) fails with "network … not
        # found" until systemd gives up. Each one checks its own networks right
        # before `docker run` and creates missing ones (locked, the same script).
        (lib.mapAttrs' (
            _: container:
              lib.nameValuePair container.serviceName {
                after = ["neo-docker-network.service"];
                wants = ["neo-docker-network.service"];
                partOf = ["docker.service"];
                preStart = lib.mkBefore ''
                  for net in ${lib.escapeShellArgs (lib.filter (n: n != "host" && n != "none" && !(lib.hasPrefix "container:" n)) (docker.containerNetworks container))}; do
                    if ! ${dockerBin} network inspect "$net" >/dev/null 2>&1; then
                      echo "network $net missing, creating the app networks"
                      ${pkgs.util-linux}/bin/flock /run/neo-docker-network.lock ${script} || true
                      break
                    fi
                  done
                '';
              }
          )
          containers)
      ];
    };
  };
}
