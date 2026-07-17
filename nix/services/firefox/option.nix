# LinuxServer Firefox (browser-accessible Firefox via Selkies) service options.
{...}: {
  flake.modules.nixos.firefox-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.firefox = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Firefox browser desktop (LinuxServer)" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 3060;
                internal = true;
                description = "Internal HTTP port for the Selkies web UI (proxied by SWAG; CUSTOM_PORT)";
              };
              title = mkOption {
                type = types.str;
                default = "Firefox";
                description = "Browser tab / Selkies UI title";
                rank = 10;
              };
              firefoxCli = mkOption {
                type = types.str;
                default = "";
                description = "Optional Firefox CLI args / start URL (FIREFOX_CLI); empty leaves image default";
                rank = 20;
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "firefox";
              auth.enabled = true;
            }
            // lib.neo.mkVpnOptions {
              containers = ["firefox"];
              networks = ["internal"];
              ports = [3060 3061 8083];
            }
            // lib.neo.mkContainerDefinitions {
              firefox = "lscr.io/linuxserver/firefox:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/firefox"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = "https://raw.githubusercontent.com/linuxserver/docker-templates/master/linuxserver.io/img/firefox-logo.png";
              description = ''
                Firefox (linuxserver/firefox) streams a full Firefox browser session to any modern browser via Selkies.
                Profile and downloads persist under appdata (/config). Neo remaps ports off the image defaults so the
                container can share the gluetun VPN namespace with webtop/karakeep without port collisions.

                Treat this as a powerful shell: passwordless sudo inside the session, so keep tinyauth (or another gate) enabled.

                Optional VPN: set services.firefox.vpn.enabled = true to route the browser container through the shared gluetun VPN (services.vpn). Off by default.
              '';
              projectUrl = "https://docs.linuxserver.io/images/docker-firefox/";
              githubUrl = "https://github.com/linuxserver/docker-firefox";
              releaseUrl = "https://github.com/linuxserver/docker-firefox/releases";
              iframeCompatible = false;
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Firefox browser desktop service configuration";
      };
    };
}
