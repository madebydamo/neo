# Collabora Online service options (office editing for Nextcloud).
{...}: {
  flake.modules.nixos.collabora-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.collabora = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Collabora real time collaboration platform for Nextcloud. Needs nextcloud to be enabled" {rank = 0;};
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "collabora";
              auth.available = false;
            }
            // lib.neo.mkContainerDefinitions {
              collabora = "collabora/code";
              extraUnits = ["collabora-setup"];
            }
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = ./icon.svg;
              description = ''
                Collabora Online is a collaborative online office suite based on LibreOffice technology.
                It provides real-time document, spreadsheet, and presentation editing in any modern browser with no plugins required.
                Self-hosted via the official CODE Docker image, it delivers frequent updates with cutting-edge features perfect for home users and small teams.
                Seamlessly integrates with Nextcloud (and other WOPI clients) for secure, private collaborative editing under your full control.
              '';
              projectUrl = "https://www.collaboraonline.com/code/";
              githubUrl = "https://github.com/CollaboraOnline/online";
              releaseUrl = "https://github.com/CollaboraOnline/online/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Collabora service configuration";
      };
    };
}
