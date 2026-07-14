# Searxng service options.
{...}: {
  flake.modules.nixos.searxng-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.searxng = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "searxng service" {rank = 0;};
            }
            // neo.mkReverseProxyOptions {
              subdomain = "search";
              auth = {
                enabled = false;
              };
            }
            // neo.mkVpnOptions {
              containers = ["searxng"];
              internalContainers = ["searxng-redis"];
              networks = ["internal"];
              ports = [8080];
            }
            // lib.neo.mkContainerDefinitions {
              "searxng-redis" = "docker.io/valkey/valkey:8-alpine";
              searxng = "docker.io/searxng/searxng:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/searxng"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/searxng.svg";
              description = ''
                SearXNG is a free internet metasearch engine which aggregates results from various search services and databases. Users are neither tracked nor profiled.
                It pulls search results from up to 244 sources while protecting your privacy and can be used over Tor for additional anonymity.
                Highly configurable with support for plugins, themes, and a simple API, SearXNG lets you search the web without being tracked by big tech companies.
                Self-hostable and actively maintained, it provides a powerful private alternative to mainstream search engines.
              '';
              projectUrl = "https://searxng.org/";
              githubUrl = "https://github.com/searxng/searxng";
              releaseUrl = "https://github.com/searxng/searxng/releases";
            };
        };
        default = {};
        description = "Searxng service configuration";
      };
    };
}
