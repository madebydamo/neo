# NetBird client options: this server joins a NetBird network.
{...}: {
  flake.modules.nixos.netbird-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      cfg = config.neo.services.netbird;
      server = config.neo.services.netbird-server or {};
      # netbird-login exists only with a setup key (module: login.enable).
      units =
        ["netbird"]
        ++ optional (cfg.setupKey != null) "netbird-login"
        ++ ["netbird-check"];
      ipv4Cidr = types.addCheck (types.strMatching "[0-9]{1,3}(\\.[0-9]{1,3}){3}/[0-9]{1,2}") lib.neo.cidr4.isNetwork;
    in {
      options.neo.services.netbird = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "NetBird, a private network that lets your own devices reach this server from anywhere" {rank = 0;};
              setupKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                # netbird up with NB_SETUP_KEY_FILE, only while the client is not registered.
                description = "Key from the NetBird dashboard (Setup Keys) that adds this server to your network. Only needed until it has joined.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Setup key";};
              };
              managementUrl = mkOption {
                type = types.nullOr types.str;
                default = null;
                # ManagementURL in config.json and --management-url at login.
                description = "Address of the NetBird network to join, e.g. https://netbird.example.com. Empty uses NetBird server on this machine when it is on, otherwise NetBird's own service at netbird.io.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Network address";};
              };
              networkRange = mkOption {
                type = ipv4Cidr;
                default = server.networkRange or "10.213.0.0/16";
                defaultText = "NetBird server's range, or 10.213.0.0/16";
                # SWAG's ingress class "netbird" and the netbird-check comparison with wt0.
                description = "The addresses your NetBird network gives its devices, shown in its dashboard under Settings → Networks. Neo uses it to recognise your NetBird devices. The default matches a network hosted by Neo.";
                rank = 30;
                ui = lib.neo.ui.mkUi {label = "Network address range";};
              };
              acceptDns = mkOption {
                type = types.bool;
                default = true;
                # DisableDNS = false. The resolver listens on 127.0.0.153:53.
                description = "Use the DNS settings of your NetBird network, such as device names and app names. Only one of Use Tailscale DNS and Use NetBird DNS can be on.";
                rank = 40;
                ui = lib.neo.ui.mkUi {label = "Use NetBird DNS";};
              };
              appNames = mkOption {
                type = types.bool;
                default = false;
                # A DNS zone for services.swag.domain in the control plane, wildcard and apex
                # A records → this server's NetBird address. netbird-check verifies it.
                description = "Your devices on NetBird reach your apps by name, directly over NetBird. Needs a DNS zone for your domain in the NetBird dashboard that points at this server.";
                rank = 50;
                ui = lib.neo.ui.mkUi {label = "App names over NetBird";};
              };
              hostname = mkOption {
                type = types.nullOr types.str;
                default = null;
                # netbird up --hostname, at the first login only.
                description = "Name of this server in NetBird. Empty uses the server's own name. Only used when the server joins; rename it later in the dashboard.";
                rank = 60;
                ui = lib.neo.ui.mkUi {label = "Device name";};
              };
              ssh = mkOption {
                type = types.bool;
                default = false;
                # ServerSSHAllowed.
                description = "Allow SSH logins over NetBird, controlled by your network's access rules.";
                rank = 70;
                ui = lib.neo.ui.mkUi {label = "NetBird SSH";};
              };
              routing = mkOption {
                type = types.bool;
                default = false;
                # services.netbird.useRoutingFeatures: forwarding on. Routes and exit nodes
                # are chosen in the dashboard.
                description = "Let this server forward traffic for your NetBird devices, as an exit node or into your home network. Which networks it offers is chosen in the NetBird dashboard.";
                rank = 80;
                ui = lib.neo.ui.mkUi {label = "Route for the network";};
              };
              acceptRoutes = mkOption {
                type = types.bool;
                default = true;
                # DisableClientRoutes = !acceptRoutes.
                description = "Let this server reach networks and exit nodes that other NetBird devices offer.";
                rank = 85;
                ui = lib.neo.ui.mkUi {
                  label = "Use shared networks";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              port = mkOption {
                type = types.port;
                default = 51820;
                # WireGuard UDP port, opened in the firewall.
                description = "Port for direct connections between NetBird devices. Change it only if another program uses it.";
                rank = 90;
                ui = lib.neo.ui.mkUi {
                  label = "Port";
                  group = lib.neo.ui.groups.advanced;
                };
              };
            }
            // lib.neo.mkSystemdUnits units
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/netbird"
            // lib.neo.mkServiceMeta {
              category = "Network";
              license = {
                spdx = "BSD-3-Clause";
                url = "https://github.com/netbirdio/netbird/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Reach your apps from anywhere, over your own private network";
              description = ''
                Connects your phone and laptop to your homeserver through a private network, so you can use your apps while away without making them public on the internet.
                The network can run on one of your own servers with NetBird server, so no outside account is needed.
              '';
              notes = ''
                WireGuard-based mesh network. Groups, access rules, routes, exit nodes and DNS zones are set in the NetBird dashboard, not here. By default every device in the network reaches every port of this server, also ports the firewall closes elsewhere: narrow it with access rules.
                Without a Network address the client joins netbird.io, NetBird's hosted service; NetBird server replaces it with your own.
                The identity of this server (its key and registration) lives in appdata, so a restore brings it back as the same device. Switch the old machine off first: two machines with one identity fight over the connection. Clear appdata makes it leave the network; it joins again with a valid setup key.
                Apps whose Reachable from list was narrowed by hand are closed to NetBird until NetBird is ticked there.
              '';
              projectUrl = "https://netbird.io/";
              githubUrl = "https://github.com/netbirdio/netbird";
              releaseUrl = "https://github.com/netbirdio/netbird/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "NetBird client configuration";
      };
    };
}
