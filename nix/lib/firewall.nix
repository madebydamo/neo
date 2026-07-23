# Firewall helpers for host services that bind loopback but must remain
# reachable from Docker (SWAG host.docker.internal → host-gateway).
#
# Binding 127.0.0.1 blocks LAN/WAN; DNAT from docker bridges rewrites
# container→host-gateway traffic to loopback. route_localnet is set once
# below (unique sysctl option — must not be set per service).
{lib, ...}: {
  # Single definition: unique sysctl cannot be mkDefault'd from neo + hermes.
  flake.modules.nixos.docker-to-localhost-sysctl = {
    boot.kernel.sysctl."net.ipv4.conf.all.route_localnet" = true;
  };

  libExtensions.firewall = {
    neo = {
      # Ports: list of host TCP ports listening on 127.0.0.1 that SWAG (or
      # other containers) must reach via host.docker.internal.
      # Only emits mergeable firewall rules (not route_localnet).
      mkDockerToLocalhostForward = ports:
        with lib; let
          portList = toList ports;
          dnatLine = iface: port: let
            p = toString port;
          in ''
            iptables -t nat -C PREROUTING -i ${iface} -p tcp --dport ${p} -j DNAT --to-destination 127.0.0.1:${p} 2>/dev/null || \
              iptables -t nat -A PREROUTING -i ${iface} -p tcp --dport ${p} -j DNAT --to-destination 127.0.0.1:${p}
          '';
          undnatLine = iface: port: let
            p = toString port;
          in ''
            iptables -t nat -D PREROUTING -i ${iface} -p tcp --dport ${p} -j DNAT --to-destination 127.0.0.1:${p} 2>/dev/null || true
          '';
          ifaces = ["br+" "docker0"];
        in
          mkIf (portList != []) {
            networking.firewall.extraCommands =
              concatMapStrings (
                port: concatMapStrings (iface: dnatLine iface port) ifaces
              )
              portList;
            networking.firewall.extraStopCommands =
              concatMapStrings (
                port: concatMapStrings (iface: undnatLine iface port) ifaces
              )
              portList;
          };
    };
  };
}
