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
          options = {
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
          };
        };
        default = {};
        description = "VPN service configuration (gluetun)";
      };
    };
}
