# VPN (gluetun) service options.
{...}: {
  flake.modules.nixos.vpn-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.vpn = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "VPN (gluetun) service for WireGuard" {rank = 0;};

              image = mkOption {
                type = types.str;
                default = "qmcgaw/gluetun";
                description = "Docker image for gluetun VPN";
                rank = 10;
              };

              vpnServiceProvider = mkOption {
                type = types.str;
                default = "airvpn";
                description = "VPN provider for gluetun";
                rank = 20;
              };

              wireguardPrivateKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "WireGuard private key (sensitive)";
                rank = 30;
              };

              wireguardPresharedKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "WireGuard preshared key";
                rank = 40;
              };

              wireguardAddresses = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "WireGuard addresses";
                rank = 50;
              };

              serverCountries = mkOption {
                type = types.str;
                default = "Netherlands";
                description = "Server countries for VPN";
                rank = 60;
              };

              firewallVpnInputPorts = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Firewall VPN input ports";
                rank = 70;
              };
            }
            # The gluetun container. Its image stays the top-level `image`
            # option (settings key), so mkContainerDefinitions is not used.
            // lib.neo.mkSystemdUnits ["docker-vpn"]
            // lib.neo.mkServiceMeta {
              category = "Network";
              icon = ./icon.svg;
              tagline = "Send app traffic through your VPN provider";
              description = ''
                Connects to your VPN provider, such as Mullvad or Proton VPN, so that the apps you choose go online through the VPN instead of your home connection.
                Your other apps and devices are not affected.
              '';
              notes = ''
                Runs gluetun with your provider's WireGuard credentials (private key, preshared key, addresses); includes DNS-over-TLS and a kill switch.
                Services opt in with their vpn options; network aliases, dependencies and container patching are handled automatically.
              '';
              privacy = ["browsing"];
              projectUrl = "https://github.com/qdm12/gluetun-wiki";
              githubUrl = "https://github.com/passteque/gluetun";
              releaseUrl = "https://github.com/passteque/gluetun/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "VPN service configuration (gluetun)";
      };
    };
}
