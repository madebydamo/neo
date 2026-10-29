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
              icon = ./icon.png;
              tagline = "Share text snippets and files with a link";
              description = ''
                Paste some text, a screenshot or a file and get a short link to share. Handy for notes, logs or moving a file between devices.
              '';
              notes = ''
                wantguns/bin: stores flat files, no database, tiny static binary.
                Accepts text, images from the clipboard and dropped files; has a CLI client, Vim integration, syntax highlighting and a small REST API.
              '';
              replaces = ["Pastebin.com"];
              githubUrl = "https://github.com/wantguns/bin";
              releaseUrl = "https://github.com/wantguns/bin/releases";
              iframeCompatible = false;
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Pastebin service configuration";
      };
    };
}
