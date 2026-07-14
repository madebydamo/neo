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
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional volume mounts";
                rank = 10;
              };
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
                Mount additional host paths via additionalMountPoints.
              '';
              projectUrl = "https://filebrowser.org/";
              githubUrl = "https://github.com/filebrowser/filebrowser";
            };
        };
        default = {};
        description = "Filebrowser service configuration";
      };
    };
}
