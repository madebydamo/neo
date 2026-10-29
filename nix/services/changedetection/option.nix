# Changedetection service options.
# Web UI protected with tinyauth forward auth (enabled by default).
{...}: {
  flake.modules.nixos.changedetection-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.changedetection = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Alerts when a web page changes, e.g. a price drop or a product back in stock" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 5000;
                description = "Internal port for changedetection web UI";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "changedetection";
            }
            // lib.neo.mkVpnOptions {
              containers = ["changedetection" "changedetection-webengine"];
              networks = ["internal"];
              ports = [5000 4444];
            }
            // lib.neo.mkContainerDefinitions {
              changedetection = "dgtlmoon/changedetection.io";
              "changedetection-webengine" = "selenium/standalone-chrome-debug:3.141.59";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/changedetection"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.svg;
              tagline = "Get a message when a website changes";
              description = ''
                Watches web pages for you and tells you when something changes: a price drops, a product is back in stock, a new version is released or an announcement goes up.
                Messages can go to email, Telegram, Discord and many other services.
              '';
              notes = ''
                Changedetection.io. Supports a visual selector for parts of a page, browser steps (log in, click, search), JSONPath/XPath/CSS filters, PDF monitoring, proxies, schedules, optional AI summaries of changes and a REST API.
                A browser extension adds pages quickly.
              '';
              replaces = ["Visualping" "Distill"];
              projectUrl = "https://changedetection.io/";
              githubUrl = "https://github.com/dgtlmoon/changedetection.io";
              releaseUrl = "https://github.com/dgtlmoon/changedetection.io/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Changedetection service configuration";
      };
    };
}
