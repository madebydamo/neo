# Nextcloud service options. Web UI protected with tinyauth forward auth.
{...}: {
  flake.modules.nixos.nextcloud-option = {
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
              icon = "https://cdn.jsdelivr.net/gh/simple-icons/simple-icons/icons/collaboraonline.svg";
              description = ''
                Collabora Online is a collaborative online office suite based on LibreOffice technology.
                It provides real-time document, spreadsheet, and presentation editing in any modern browser with no plugins required.
                Self-hosted via the official CODE Docker image, it delivers frequent updates with cutting-edge features perfect for home users and small teams.
                Seamlessly integrates with Nextcloud (and other WOPI clients) for secure, private collaborative editing under your full control.
              '';
              projectUrl = "https://www.collaboraonline.com/code/";
              githubUrl = "https://github.com/CollaboraOnline/online";
              releaseUrl = "https://github.com/CollaboraOnline/online/releases";
            };
        };
        default = {};
        description = "Collabora service configuration";
      };
    };
}
