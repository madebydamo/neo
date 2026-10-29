# Tailscale service options.
{...}: {
  flake.modules.nixos.tailscale-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      # splitDns units are only defined under mkIf cfg.splitDns. Listing them
      # unconditionally makes service-targets apply PartOf= and synthesize
      # empty stubs (LoadState=bad-setting) when splitDns is off (default).
      # Same pattern as ical-sync: only declare units that actually exist.
      tailscaleUnits =
        ["tailscale-up" "tailscaled"]
        ++ optionals (config.neo.services.tailscale.splitDns or false) [
          "tailscale-split-dns"
          "dnsmasq"
        ];
    in {
      options.neo.services.tailscale = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Tailscale, a private network that lets your own devices reach this server from anywhere" {rank = 0;};
              authKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                # Used by tailscale up --auth-key only while the node is not logged in.
                description = "Key from the Tailscale admin console that signs this server in automatically.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Auth key";};
              };
              acceptDns = mkOption {
                type = types.bool;
                default = true;
                description = "Use the DNS settings (such as MagicDNS names) from your Tailscale admin console.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Use Tailscale DNS";};
              };
              splitDns = mkOption {
                type = types.bool;
                default = false;
                # Mini dnsmasq on the Tailscale interface answering Neo hostnames with this node's
                # Tailscale IP. Admin console: add this node's Tailscale IPv4 as a nameserver
                # restricted to services.swag.domain (do not enable Override DNS servers).
                # With Pi-hole enabled, Pi-hole binds to localIP only so both can use port 53.
                description = "Let devices on Tailscale reach your apps by name. Needs one DNS setting in the Tailscale admin console: this server as nameserver for your domain.";
                rank = 25;
                ui = lib.neo.ui.mkUi {label = "App names over Tailscale";};
              };
              acceptRoutes = mkOption {
                type = types.bool;
                default = false;
                description = "Let this server reach networks that other Tailscale devices share.";
                rank = 30;
                ui = lib.neo.ui.mkUi {
                  label = "Use shared networks";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              advertiseExitNode = mkOption {
                type = types.bool;
                default = false;
                description = "Let your Tailscale devices send their internet traffic through this server, e.g. on public Wi-Fi.";
                rank = 40;
                ui = lib.neo.ui.mkUi {label = "Offer as exit node";};
              };
              advertiseRoutes = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Networks this server shares with your Tailscale devices, e.g. 192.168.0.0/24 for your home network.";
                rank = 50;
                ui = lib.neo.ui.mkUi {
                  label = "Share networks";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              exitNode = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Send this server's internet traffic through another Tailscale device (its name or IP). Empty uses the normal connection.";
                rank = 60;
                ui = lib.neo.ui.mkUi {label = "Use exit node";};
              };
              exitNodeAllowLanAccess = mkOption {
                type = types.bool;
                default = false;
                description = "Keep reaching your home network directly while using an exit node.";
                rank = 70;
                ui = lib.neo.ui.mkUi {
                  label = "Allow home network access";
                  visibleWhen = "exitNode";
                };
              };
              hostname = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Name of this server in Tailscale. Empty uses the server's own name.";
                rank = 80;
                ui = lib.neo.ui.mkUi {label = "Device name";};
              };
              loginServer = mkOption {
                type = types.str;
                default = "https://controlplane.tailscale.com";
                description = "Only change this if you run your own Headscale server.";
                rank = 90;
                ui = lib.neo.ui.mkUi {
                  label = "Login server";
                  group = lib.neo.ui.groups.advanced;
                };
              };
              ssh = mkOption {
                type = types.bool;
                default = false;
                description = "Allow SSH logins over Tailscale, controlled by your Tailscale access rules.";
                rank = 100;
                ui = lib.neo.ui.mkUi {label = "Tailscale SSH";};
              };
            }
            // lib.neo.mkSystemdUnits tailscaleUnits
            // lib.neo.mkServiceMeta {
              category = "Network";
              icon = ./icon.svg;
              tagline = "Reach your apps from anywhere, privately";
              description = ''
                Connects your phone and laptop to your homeserver through a private network, so you can use your apps while away without making them public on the internet.
                Install the Tailscale app on your devices and sign in.
              '';
              notes = ''
                WireGuard-based mesh VPN with NAT traversal and identity-based access rules. Supports exit nodes, advertised routes, MagicDNS and SSH gated by tailnet policy.
                With splitDns your domain resolves to this server on Tailscale devices (set Pi-hole's localIP as well).
              '';
              projectUrl = "https://tailscale.com/";
              githubUrl = "https://github.com/tailscale/tailscale";
              releaseUrl = "https://github.com/tailscale/tailscale/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Tailscale service configuration";
      };
    };
}
