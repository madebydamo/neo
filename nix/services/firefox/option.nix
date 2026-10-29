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
              icon = ./icon.png;
              tagline = "A Firefox browser that runs on your server";
              description = ''
                A complete Firefox that you open inside a browser tab.
                Useful to browse through your home connection or VPN while you are away, or to keep a browser session running after you close the tab.
              '';
              notes = ''
                linuxserver/firefox, streamed with Selkies. Profile and downloads persist under appdata (/config).
                Neo moves the ports off the image defaults so the container can share the VPN network with webtop and karakeep.
                Treat it as a powerful shell: there is passwordless sudo inside the session, so keep tinyauth (or another login) enabled.
                Optional VPN: turn on vpn.enabled to route the browser through the shared VPN service. Off by default.
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
