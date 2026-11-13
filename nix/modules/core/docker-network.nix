# Docker networks for the apps.
#
# `internal`: every app container and SWAG sit on it (IPv4).
#
# `swag-ipv6`: IPv6 only, SWAG alone. Without an IPv6 network Docker hands an
# IPv6 connection to a published port to its userland proxy, which reconnects
# over IPv4 from the gateway: SWAG saw every IPv6 visitor as 172.18.0.1, a
# "Home network" address. With an IPv6 endpoint Docker forwards the connection
# and keeps its source address. The apps stay on IPv4: most listen on IPv4 only,
# and an IPv6 address in Docker's DNS would send their clients there first.
#
# `neo.core.dockerNetworks`: more IPv4 networks a module needs for itself,
# e.g. `monitoring` (Grafana, Loki, Prometheus and SWAG only).
{...}: {
  flake.modules.nixos.docker-network = {
    config,
    lib,
    pkgs,
    ...
  }: let
    docker = "${config.virtualisation.docker.package}/bin/docker";
    ipv6Net = lib.neo.swagIpv6Network;
    script = pkgs.writeShellScript "neo-docker-network" ''
      set -euo pipefail
      exists() { ${docker} network inspect "$1" >/dev/null 2>&1; }

      exists internal || ${docker} network create internal >/dev/null
      ${lib.concatMapStrings (n: ''
          exists ${n} || ${docker} network create ${n} >/dev/null
        '')
        config.neo.core.dockerNetworks}

      if ! exists ${ipv6Net}; then
        if ! ${docker} network create --ipv6 --ipv4=false ${ipv6Net} >/dev/null; then
          # No IPv6 on this host. SWAG still joins the network, which then does nothing.
          echo "IPv6 network not available, SWAG gets IPv4 only" >&2
          ${docker} network create ${ipv6Net} >/dev/null
        fi
      fi
    '';
  in {
    options.neo.core.dockerNetworks = lib.mkOption {
      type = lib.types.listOf (lib.types.strMatching "[a-z][a-z0-9-]*");
      default = [];
      internal = true;
      description = "Extra Docker networks to create next to internal (one per isolated group of containers)";
    };

    config.systemd.services = lib.mkMerge [
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
        config.virtualisation.oci-containers.containers)
    ];
  };
}
