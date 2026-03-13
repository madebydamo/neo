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
              enabled = mkEnableOption (lib.mdDoc "filebrowser service");
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = lib.mdDoc "Additional volume mounts";
              };
              domain = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = lib.mdDoc "Primary domain for swag";
              };
              email = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = lib.mdDoc "LetsEncrypt email for swag";
              };
              extraDomains = mkOption {
                type = types.listOf types.str;
                default = [];
                description = lib.mdDoc "Extra domains for swag";
              };
            }
            // neo.mkReverseProxyOptions {
              subdomain = "filebrowser";
              auth.publicPaths = [
                "^\\/api\\/public\\/"
                "^\\/share\\/"
              ];
            };
        };
        default = {};
        description = lib.mdDoc "Filebrowser service configuration";
      };
    };
}
