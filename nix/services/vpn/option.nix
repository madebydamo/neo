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
              enabled = mkEnableOption "a VPN connection that chosen apps send their internet traffic through" {rank = 0;};

              image = mkOption {
                type = types.str;
                default = "qmcgaw/gluetun";
                description = "Container image of the VPN client (gluetun).";
                rank = 10;
                ui = lib.neo.ui.mkUi {
                  label = "Image";
                  group = lib.neo.ui.groups.advanced;
                };
              };

              vpnServiceProvider = mkOption {
                type = types.str;
                default = "airvpn";
                # VPN_SERVICE_PROVIDER (gluetun provider id)
                description = "Your VPN company, e.g. mullvad, protonvpn, airvpn.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "VPN provider";};
              };

              wireguardPrivateKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "The PrivateKey line from your provider's WireGuard config.";
                rank = 30;
                ui = lib.neo.ui.mkUi {label = "Private key";};
              };

              wireguardPresharedKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "The PresharedKey line from your provider's WireGuard config.";
                rank = 40;
                ui = lib.neo.ui.mkUi {label = "Preshared key";};
              };

              wireguardAddresses = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "The Address line from your provider's WireGuard config, e.g. 10.64.1.2/32.";
                rank = 50;
                ui = lib.neo.ui.mkUi {label = "Address";};
              };

              serverCountries = mkOption {
                type = types.str;
                default = "Netherlands";
                description = "Countries of the VPN servers to use, comma-separated, e.g. Netherlands,Switzerland.";
                rank = 60;
                ui = lib.neo.ui.mkUi {label = "Server countries";};
              };

              firewallVpnInputPorts = mkOption {
                type = types.nullOr types.str;
                default = null;
                # FIREWALL_VPN_INPUT_PORTS
                description = "Ports your VPN provider forwards to you, comma-separated (e.g. for seeding torrents). Empty opens none.";
                rank = 70;
                ui = lib.neo.ui.mkUi {label = "Forwarded ports";};
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
