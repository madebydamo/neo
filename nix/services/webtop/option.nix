# LinuxServer Webtop (browser-accessible full desktop) service options.
# Default image is Ubuntu XFCE; switch distro/DE via containers.webtop tag.
{...}: {
  flake.modules.nixos.webtop-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.webtop = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "a full Linux desktop that runs in your browser" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 3050;
                internal = true;
                description = "Internal HTTP port for the Selkies web desktop (proxied by SWAG; CUSTOM_PORT)";
              };
              title = mkOption {
                type = types.str;
                default = "Webtop";
                # TITLE (Selkies UI title)
                description = "Name shown in the browser tab.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Tab title";};
              };
            }
            // lib.neo.mkAdditionalMountPoints {
              rank = 20;
              description = "Server folders to make available inside the desktop. Each entry pairs a folder on the server with where it appears in the desktop.";
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "webtop";
              auth.enabled = true;
            }
            // lib.neo.mkVpnOptions {
              containers = ["webtop"];
              networks = ["internal"];
              ports = [3050 3051 8082];
            }
            // lib.neo.mkContainerDefinitions {
              # Lightweight Ubuntu + XFCE. Other distros/DEs: change the tag only
              # (see service description and neo-webtop skill). Examples:
              #   lscr.io/linuxserver/webtop:ubuntu-i3
              #   lscr.io/linuxserver/webtop:alpine-xfce (tag: latest)
              #   lscr.io/linuxserver/webtop:debian-kde
              #   lscr.io/linuxserver/webtop:fedora-mate
              #   lscr.io/linuxserver/webtop:arch-xfce
              webtop = "lscr.io/linuxserver/webtop:ubuntu-xfce";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/webtop"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.png;
              tagline = "A full Linux desktop in your browser";
              description = ''
                A complete desktop computer that runs on your server and opens in a browser tab, from any device.
                Your files and settings stay there between sessions.
              '';
              notes = ''
                linuxserver/webtop streamed with Selkies; Neo defaults to Ubuntu XFCE (tag ubuntu-xfce). The home directory persists under appdata.
                Switch distro or desktop by changing only the image tag under containers.webtop, e.g. ubuntu-i3, ubuntu-mate, ubuntu-kde, debian-xfce, fedora-kde, arch-xfce, alpine-i3 or latest (Alpine XFCE). Full list: https://docs.linuxserver.io/images/docker-webtop/
                After changing the tag, apply again. Packages installed with apt/dnf do not survive a container recreate; use proot-apps inside the desktop for persistent apps.
                Treat it as a powerful shell: there is passwordless sudo inside the desktop, so keep tinyauth (or another login) enabled.
                Optional VPN: turn on vpn.enabled to route the desktop through the shared VPN service. Off by default.
              '';
              projectUrl = "https://docs.linuxserver.io/images/docker-webtop/";
              githubUrl = "https://github.com/linuxserver/docker-webtop";
              releaseUrl = "https://github.com/linuxserver/docker-webtop/releases";
              iframeCompatible = false;
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Webtop browser desktop service configuration";
      };
    };
}
