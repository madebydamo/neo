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
              enabled = mkEnableOption "Collabora online office: edit documents, spreadsheets and presentations together in Nextcloud (needs Nextcloud)" {rank = 0;};
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
              license = {
                spdx = "MPL-2.0";
                url = "https://github.com/CollaboraOnline/online/blob/main/COPYING";
              };
              icon = ./icon.svg;
              tagline = "Edit office documents together in Nextcloud";
              description = ''
                Open and edit Word, Excel and PowerPoint files directly in Nextcloud, together with others at the same time.
                Works in any browser, no Office installation needed.
              '';
              notes = ''
                Runs the official CODE image (based on LibreOffice). Needs Nextcloud, which talks to it over WOPI.
              '';
              replaces = ["Google Docs" "Microsoft 365"];
              privacy = ["documents"];
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
