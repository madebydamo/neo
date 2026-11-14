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
    legacyNetworks = ["monitoring"];

    ipv4Networks = lib.filterAttrs (n: _: n != ipv6Net) cfg.networks;

    script = pkgs.writeShellScript "neo-docker-network" ''
      set -euo pipefail
      docker=${dockerBin}
      exists() { "$docker" network inspect "$1" >/dev/null 2>&1; }
      in_use() { [ "$("$docker" network inspect -f '{{len .Containers}}' "$1")" != 0 ]; }

      # ensure NAME INTERNAL(true|false) SUBNET GATEWAY BRIDGE (last three may be "")
      ensure() {
        local name=$1 internal=$2 subnet=$3 gateway=$4 bridge=$5 want current
        local cur_internal cur_subnet cur_bridge
        want="$internal|$subnet|$bridge"
        if exists "$name"; then
          # Not created by Neo (internal, a plugin's own network): never touch it.
          if [ "$("$docker" network inspect -f '{{index .Labels "neo.managed"}}' "$name")" != 1 ]; then
            return 0
          fi
          IFS='|' read -r cur_internal cur_subnet cur_bridge < <(
            "$docker" network inspect -f '{{.Internal}}|{{range .IPAM.Config}}{{.Subnet}}{{end}}|{{index .Options "com.docker.network.bridge.name"}}' "$name"
          )
          # An empty subnet / bridge name means "any".
          [ -z "$subnet" ] && cur_subnet=
          [ -z "$bridge" ] && cur_bridge=
          current="$cur_internal|$cur_subnet|$cur_bridge"
          [ "$current" = "$want" ] && return 0
          if in_use "$name"; then
            echo "network $name: is $current, should be $want; recreated once its containers stop" >&2
            return 0
          fi
          "$docker" network rm "$name" >/dev/null
        fi
        local args=(--label neo.managed=1)
        [ "$internal" = true ] && args+=(--internal)
        [ -n "$subnet" ] && args+=(--subnet "$subnet")
        [ -n "$gateway" ] && args+=(--gateway "$gateway")
        [ -n "$bridge" ] && args+=(-o "com.docker.network.bridge.name=$bridge")
        if "$docker" network create "''${args[@]}" "$name" >/dev/null; then
          echo "network $name: created"
        else
          echo "network $name: could not be created ($want)" >&2
          failed=1
        fi
      }
      failed=0

      # Fixed subnets first, before the pool hands their range to another network.
      ${lib.concatStrings (map (name: let
          net = ipv4Networks.${name};
          arg = v: lib.escapeShellArg (toString (lib.defaultTo "" v));
        in ''
          ensure ${lib.escapeShellArg name} ${lib.boolToString net.internal} ${arg net.subnet} ${arg net.gateway} ${arg net.bridgeName}
        '')
        (lib.sort (a: b: (ipv4Networks.${a}.subnet != null) && (ipv4Networks.${b}.subnet == null)) (lib.attrNames ipv4Networks)))}

      if ! exists ${ipv6Net}; then
        if ! "$docker" network create --label neo.managed=1 --ipv6 --ipv4=false ${ipv6Net} >/dev/null; then
          # No IPv6 on this host. SWAG still joins the network, which then does nothing.
          echo "IPv6 network not available, SWAG gets IPv4 only" >&2
          "$docker" network create --label neo.managed=1 ${ipv6Net} >/dev/null
        fi
      fi

      # Remove networks no one declares any more, once empty.
      declared=" ${lib.concatStringsSep " " (lib.attrNames cfg.networks)} "
      {
        "$docker" network ls --filter label=neo.managed=1 --format '{{.Name}}'
        ${lib.concatMapStrings (n: ''
          if exists ${n}; then echo ${n}; fi
        '')
        legacyNetworks}
      } | sort -u | while read -r name; do
        [ -n "$name" ] || continue
        [ "$name" = ${docker.sharedNetwork} ] && continue
        case "$declared" in *" $name "*) continue ;; esac
        if in_use "$name"; then
          echo "network $name: no longer declared, kept while containers use it" >&2
        elif "$docker" network rm "$name" >/dev/null 2>&1; then
          echo "network $name: removed"
        fi
      done
      exit "$failed"
    '';
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
      };
      dockerNetworks = lib.mkOption {
        type = lib.types.listOf (lib.types.strMatching "[a-z][a-z0-9-]*");
        default = [];
        internal = true;
        description = "Deprecated: extra Docker networks to create. Use neo.core.docker.networks, or just name the network in a container's networks.";
      };
    };

    config = {
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

      assertions = lib.flatten (lib.mapAttrsToList (link: members:
        map (c: {
          assertion = containers ? ${c} && lib.elem (docker.linkNetwork link) (docker.containerNetworks containers.${c});
          message = "Docker link ${link}: container ${c} does not exist, or does not join ${docker.linkNetwork link} (a VPN-routed container shares gluetun's network and cannot join a link).";
        })
        members)
      cfg.links);

      systemd.services = lib.mkMerge [
        {
          neo-docker-network = {
            description = "Docker networks for the apps";
            after = ["docker.service"];
            requires = ["docker.service"];
            wantedBy = ["multi-user.target"];
            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
              ExecStart = script;
            };
          };
        }
        # Wants, not Requires: a restart of this unit (a new script) must not
        # restart every container.
        (lib.mapAttrs' (
            _: container:
              lib.nameValuePair container.serviceName {
                after = ["neo-docker-network.service"];
                wants = ["neo-docker-network.service"];
              }
          )
          containers)
      ];
    };
  };
}
