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
              enabled = mkEnableOption "webtop browser desktop (LinuxServer)" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 3050;
                internal = true;
                description = "Internal HTTP port for the Selkies web desktop (proxied by SWAG; CUSTOM_PORT)";
              };
              title = mkOption {
                type = types.str;
                default = "Webtop";
                description = "Browser tab / Selkies UI title";
                rank = 10;
              };
            }
            // lib.neo.mkAdditionalMountPoints {
              rank = 20;
              description = ''
                Extra host directories to mount into the desktop session.
                Each entry pairs a localPath (absolute host path) with a containerPath (path inside the desktop).
              '';
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
              icon = "https://raw.githubusercontent.com/linuxserver/docker-templates/master/linuxserver.io/img/webtop-logo.png";
              description = ''
                Webtop (linuxserver/webtop) is a full Linux desktop environment streamed to any modern browser via Selkies.
                Neo defaults to Ubuntu XFCE (tag ubuntu-xfce) as a lightweight, familiar desktop; the home directory persists under appdata.

                Switch distro or desktop by changing only the container image tag under services.webtop.containers.webtop
                (web UI container field, or settings.toml). Same image name, different tag — for example:
                ubuntu-i3, ubuntu-mate, ubuntu-kde, debian-xfce, fedora-kde, arch-xfce, alpine-i3, or latest (Alpine XFCE).
                Full tag list: https://docs.linuxserver.io/images/docker-webtop/

                After changing the tag, re-apply (neo activate / system rebuild) so docker-updater or the next
                container recreate pulls the new image. Desktop packages installed with apt/dnf do not survive
                container recreation; use proot-apps inside the desktop for persistent apps, or reinstall after upgrades.

                Treat this as a powerful shell: passwordless sudo inside the desktop, so keep tinyauth (or another gate) enabled.

                Optional VPN: set services.webtop.vpn.enabled = true to route the desktop container through the shared gluetun VPN (services.vpn). Off by default.
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
