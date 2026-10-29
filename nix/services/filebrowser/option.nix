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
              icon = ./icon.png;
              tagline = "Manage the files on your server from the browser";
              description = ''
                Browse, upload, rename, preview and edit the files on your server, such as photos, documents and app data, from any browser.
              '';
              notes = ''
                Neo mounts media, documents and appdata under /srv by default; add more folders with additionalMountPoints.
              '';
              privacy = ["files"];
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
