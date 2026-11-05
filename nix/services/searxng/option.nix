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
              enabled = mkEnableOption "private web search that doesn't track you" {rank = 0;};
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
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/searxng/searxng/blob/master/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Private web search that doesn't track you";
              description = ''
                Search the web without being tracked or profiled. It asks many search engines at once and shows the combined results without ads.
                Set it as the default search engine in your browser.
              '';
              notes = ''
                Metasearch engine with results from many sources; supports plugins, themes and a simple API.
              '';
              replaces = ["Google Search"];
              privacy = ["search"];
              projectUrl = "https://searxng.org/";
              githubUrl = "https://github.com/searxng/searxng";
              releaseUrl = "https://github.com/searxng/searxng/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Searxng service configuration";
      };
    };
}
