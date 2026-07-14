# Pastebin (wantguns/bin) service options.
# Tinyauth forward auth is disabled by default.
{...}: {
  flake.modules.nixos.pastebin-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.pastebin = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "pastebin (wantguns/bin) service" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 6163;
                internal = true;
                description = "Internal port the pastebin service listens on";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "pastebin";
              auth.enabled = false;
            }
            // lib.neo.mkContainerDefinitions {
              pastebin = "wantguns/bin";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/pastebin"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = "https://raw.githubusercontent.com/wantguns/bin/master/static/media/android-chrome-512x512.png";
              description = ''
                Bin (wantguns/bin) is a highly opinionated, minimal self-hosted pastebin that accepts both textual pastes and binary files such as images and PDFs.
                It requires no SQL database or external services; all data is stored as flat files, and the server is distributed as a tiny statically linked binary (Docker image is based on scratch) for the simplest possible deployment.
                The web UI supports pasting plain text, images via clipboard, and files via drag-and-drop; it also provides a CLI client, (Neo)Vim integration, server-side syntax highlighting, and a minimal REST API.
              '';
              githubUrl = "https://github.com/wantguns/bin";
              releaseUrl = "https://github.com/wantguns/bin/releases";
              iframeCompatible = false;
            };
        };
        default = {};
        description = "Pastebin service configuration";
      };
    };
}
