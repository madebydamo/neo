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
              enabled = mkEnableOption "A file manager in your browser to upload, preview, rename and share your server's files" {rank = 0;};
            }
            # Entries bind-mount localPath (host) at containerPath (inside the
            # container); media, documents and appdata are under /srv by default.
            // lib.neo.mkAdditionalMountPoints {
              rank = 10;
              app = "Filebrowser";
              example = "/srv/Holiday photos";
              description = "More folders from your server to show in Filebrowser, next to Media, Documents and app data.";
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
                Neo mounts media, documents and appdata under /srv by default; add more under Extra folders.
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
