# Filebrowser service options.
{...}: {
  flake.modules.nixos.filebrowser-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.filebrowser = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "filebrowser service" {rank = 0;};
            }
            // lib.neo.mkAdditionalMountPoints {
              rank = 10;
              description = ''
                Extra host directories to expose in Filebrowser (beyond the default media, documents, and appdata mounts under /srv).
                Each entry pairs a localPath (absolute host path) with a containerPath (e.g. /srv/MyShare).
              '';
            }
            // neo.mkReverseProxyOptions {
              subdomain = "filebrowser";
              auth.publicPaths = [
                "^/share/"
                "^/static/"
                "^/api/public"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              filebrowser = "filebrowser/filebrowser:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/filebrowser"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = "https://filebrowser.org/static/logo.png";
              description = ''
                Filebrowser provides a file management interface.
                It lets you upload, delete, preview, rename and edit your files in the browser.
                Neo mounts media, documents, and appdata under /srv by default; add more paths via additionalMountPoints.
              '';
              projectUrl = "https://filebrowser.org/";
              githubUrl = "https://github.com/filebrowser/filebrowser";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Filebrowser service configuration";
      };
    };
}
