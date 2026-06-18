# Example service options.
{...}: {
  flake.modules.nixos.example-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.example = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Example service" {rank = 0;};
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Additional volume mounts";
                rank = 10;
              };
            }
            // neo.mkReverseProxyOptions {
              subdomain = "example";
              auth.publicPaths = [
                "^/share/"
                "^/static/"
                "^/api/public"
              ];
            }
            // neo.mkVpnOptions {
              containers = ["example"];
              networks = ["internal"];
              ports = [8080];
            }
            // lib.neo.mkServiceMeta {
              icon = "https://filebrowser.org/static/logo.png";
              description = ''
                Example service. Replace with your own description.
                It provides some functionality behind the reverse proxy and optional VPN.
              '';
              projectUrl = "https://example.com/";
              githubUrl = "https://github.com/example/example";
            };
        };
        default = {};
        description = "Example service configuration";
      };
    };
}
