# VPN (gluetun) service options.
{...}: {
  flake.modules.nixos.vpn-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.vpn = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "VPN (gluetun) service for WireGuard";

              image = mkOption {
                type = types.str;
                default = "qmcgaw/gluetun";
                description = "Docker image for gluetun VPN";
              };

              vpnServiceProvider = mkOption {
                type = types.str;
                default = "airvpn";
                description = "VPN provider for gluetun";
              };

              wireguardPrivateKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "WireGuard private key (sensitive)";
              };

              wireguardPresharedKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "WireGuard preshared key";
              };

              wireguardAddresses = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "WireGuard addresses";
              };

              serverCountries = mkOption {
                type = types.str;
                default = "Netherlands";
                description = "Server countries for VPN";
              };

              firewallVpnInputPorts = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Firewall VPN input ports";
              };
            }
            // lib.neo.mkServiceMeta {
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/gluetun.svg";
              description = ''
                Gluetun is a lightweight Swiss-army-knife VPN client in a thin Docker container supporting dozens of providers via OpenVPN or WireGuard, with built-in DNS-over-TLS, kill switch firewall, and HTTP/SOCKS/Shadowsocks proxies.
                In the Neo homeserver, this VPN service runs the gluetun container to provide a shared outbound WireGuard tunnel; other services can opt their containers into routing through it (using the `vpn` submodule options and `mkVpnOptions` helper) for privacy, unblocking, or compliance without impacting the host network or non-VPN services.
                It automatically handles network aliasing, dependsOn, and container patching for opted-in services via the docker-networks and vpn modules; requires your provider's WireGuard credentials (private key, preshared key, addresses).
              '';
              projectUrl = "https://github.com/qdm12/gluetun-wiki";
              githubUrl = "https://github.com/passteque/gluetun";
              releaseUrl = "https://github.com/passteque/gluetun/releases";
            };
        };
        default = {};
        description = "VPN service configuration (gluetun)";
      };
    };
}
