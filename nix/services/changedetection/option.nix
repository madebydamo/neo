# Changedetection service options.
# Web UI protected with tinyauth forward auth (enabled by default).
{...}: {
  flake.modules.nixos.changedetection-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.changedetection = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "changedetection.io website change detection service";
              port = mkOption {
                type = types.port;
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
            // lib.neo.mkServiceMeta {
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/changedetection.svg";
              description = ''
                Changedetection.io is the best and simplest open-source tool for website change detection, web page monitoring, and website change alerts.
                Monitor any URL for content, price, or structural changes and get notified instantly via Discord, Email, Slack, Telegram, Webhooks, and over 85 other services.
                Powerful features include AI/LLM-powered smart filtering and plain-language change summaries, Visual Selector for targeting specific elements, browser automation steps (login, click, search), JSONPath/jq/XPath/CSS filters, PDF monitoring, proxies, schedules, and a full REST API.
                Ideal for restock alerts, price drop tracking, release monitoring, defacement detection, data journalism, and keeping tabs on any dynamic web content.
                Self-hosted via Docker with easy setup and a Chrome extension for quick adding of watches.
              '';
              projectUrl = "https://changedetection.io/";
              githubUrl = "https://github.com/dgtlmoon/changedetection.io";
              releaseUrl = "https://github.com/dgtlmoon/changedetection.io/releases";
            };
        };
        default = {};
        description = "Changedetection service configuration";
      };
    };
}
