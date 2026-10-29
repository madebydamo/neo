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
              icon = ./icon.svg;
              tagline = "Keep an eye on your server's health";
              description = ''
                Shows how busy your server is: processor, memory, disk space, network and temperature, also per app.
                It warns you when a disk runs full or something overheats, and keeps a history so you can see what happened.
              '';
              notes = ''
                A hub (web UI on PocketBase) collects data from small agents over a websocket; monitored machines need no public ports.
                Turn on beszel-agent here, and install the agent on other machines you want to watch.
                Supports several users, OAuth/OIDC login, S.M.A.R.T. disk monitoring, automatic backups and a REST API.
              '';
              projectUrl = "https://beszel.dev/";
              githubUrl = "https://github.com/henrygd/beszel";
              releaseUrl = "https://github.com/henrygd/beszel/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Beszel hub service configuration";
      };
    };
}
