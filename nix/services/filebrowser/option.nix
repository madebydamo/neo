# Filebrowser service options.
{...}: {
  flake.modules.nixos.filebrowser-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.filebrowser = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "filebrowser service";
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional volume mounts";
              };
              domain = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Primary domain for swag";
              };
              email = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "LetsEncrypt email for swag";
              };
              extraDomains = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Extra domains for swag";
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
            // lib.neo.mkServiceMeta {
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
