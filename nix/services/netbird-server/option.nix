# NetBird control plane options (netbirdio/netbird-server + dashboard).
{...}: {
  flake.modules.nixos.netbird-server-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      ipv4Cidr = types.addCheck (types.strMatching "[0-9]{1,3}(\\.[0-9]{1,3}){3}/[0-9]{1,2}") lib.neo.cidr4.isNetwork;
    in {
      options.neo.services.netbird-server = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "your own private network between your devices, with no outside account" {rank = 0;};
              ownerEmail = mkOption {
                type = types.nullOr types.str;
                default = null;
                # POST /api/setup on the first start.
                description = "E-mail address of the first administrator, used to sign in to the NetBird dashboard.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Administrator e-mail";};
              };
              ownerPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Password of the first administrator. Only used when the network is created; change it later in the dashboard, not here.";
                rank = 20;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Administrator password";};
              };
              relaySecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                # server.authSecret: shared by management and the built-in relay.
                description = "Protects the relay that passes traffic between devices that cannot connect directly. Changing it disconnects every device until it reconnects.";
                rank = 30;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Relay secret";};
              };
              dataKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                # store.encryptionKey: 32 bytes, base64.
                description = "Encrypts setup keys and access tokens in the network's database. If it changes or is lost, they have to be created again.";
                rank = 40;
                helper = lib.neo.helpers.randomKeyBase64;
                ui = lib.neo.ui.mkUi {label = "Database key";};
              };
              sessionKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                # auth.sessionCookieEncryptionKey.
                description = "Protects dashboard logins. Changing it signs everyone out of the dashboard.";
                rank = 50;
                helper = lib.neo.helpers.randomToken32Hex;
                ui = lib.neo.ui.mkUi {label = "Login session key";};
              };
              enrolThisServer = mkOption {
                type = types.bool;
                default = true;
                # The bootstrap creates a one-off setup key and runs netbird up
                # when services.netbird is on and has no setup key of its own.
                description = "Add this server to its own network when the network is created. Needs NetBird turned on as well.";
                rank = 60;
                ui = lib.neo.ui.mkUi {label = "Join this server";};
              };
              networkRange = mkOption {
                type = ipv4Cidr;
                default = "10.213.0.0/16";
                # network_range of the account, written by the bootstrap before any device joins.
                description = "The addresses the network gives its devices. Set when the network is created; a later change here does nothing. Change it in the dashboard and in NetBird's Network address range together, and every device gets a new address.";
                rank = 70;
                ui = lib.neo.ui.mkUi {
                  label = "Network address range";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              stunPort = mkOption {
                type = types.port;
                default = 3478;
                # UDP, published directly (not through SWAG or streamproxy).
                description = "UDP port devices use to find a direct way to each other. It must be reachable from the internet.";
                rank = 80;
                ui = lib.neo.ui.mkUi {
                  label = "STUN port";
                  group = lib.neo.ui.groups.advanced;
                };
              };
            }
            # Devices, the API and the dashboard's own login cannot sit behind tinyauth.
            // lib.neo.mkReverseProxyOptions {
              subdomain = "netbird";
              auth.available = false;
            }
            // lib.neo.mkContainerDefinitions {
              # Read the release notes before an update; the dashboard belongs to the server version.
              netbird-server = "netbirdio/netbird-server:0.82.1";
              netbird-dashboard = "netbirdio/dashboard:v2.96.1";
              extraUnits = ["netbird-server-setup"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/netbird-server"
            // lib.neo.mkServiceMeta {
              category = "Network";
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/netbirdio/netbird/blob/main/LICENSE";
              };
              icon = ../netbird/icon.svg;
              tagline = "Your own private network between your devices";
              description = ''
                Runs the private network that your phones, laptops and servers join with NetBird, instead of using an outside service. Includes the dashboard where you add devices and decide who may reach what.
                Best run on a server with a public address, so devices can always find each other.
              '';
              notes = ''
                Combined server (management, signal, relay, STUN, built-in login) and the dashboard, behind SWAG at one address. UDP STUN port must be reachable directly; it cannot pass a TCP tunnel (rathole). Behind rathole only, devices still join but every connection is relayed.
                The first start creates the administrator, sets the address range, adds this server when NetBird is on, and creates the DNS zone for its app names when App names over NetBird is on. Later changes are made in the dashboard. /api/setup stays closed at SWAG.
                Keep Internet ticked under Reachable from: devices outside your home must reach this address to join and reconnect.
                Clear appdata deletes the whole network: every device has to join again.
              '';
              replaces = ["Tailscale" "ZeroTier"];
              projectUrl = "https://netbird.io/";
              githubUrl = "https://github.com/netbirdio/netbird";
              releaseUrl = "https://github.com/netbirdio/netbird/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "NetBird server configuration";
      };
    };
}
