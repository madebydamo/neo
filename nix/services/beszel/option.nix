# Beszel hub (server) options.
{...}: {
  flake.modules.nixos.beszel-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.beszel = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "beszel hub service" {rank = 0;};
              enableSingleUserSystem = mkOption {
                type = types.bool;
                default = true;
                description = "Disable password auth for single-user + tinyauth setup (recommended)";
                rank = 10;
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "beszel";
              auth.publicPaths = [
                "^/api"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              beszel = "henrygd/beszel:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/beszel"
            // lib.neo.mkServiceMeta {
              category = "Monitoring";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/beszel.svg";
              description = ''
                Beszel is a simple, lightweight server monitoring platform that provides Docker and Podman stats, historical metrics, and configurable alerts for CPU, memory, disk, bandwidth, temperature and more.
                It uses a central hub (web UI built on PocketBase) paired with tiny agents that report metrics from your servers and containers over websocket.
                Beszel is designed for ease of use with no requirement for public ports on monitored hosts, support for multi-user and admin sharing, OAuth / OIDC authentication, automatic backups, S.M.A.R.T. disk monitoring, and a REST API.
                It serves as a friendly, low-overhead alternative for homelab and self-hosted infrastructure visibility.
              '';
              projectUrl = "https://beszel.dev/";
              githubUrl = "https://github.com/henrygd/beszel";
              releaseUrl = "https://github.com/henrygd/beszel/releases";
            };
        };
        default = {};
        description = "Beszel hub service configuration";
      };
    };
}
