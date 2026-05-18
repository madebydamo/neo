# Beszel hub (server) options.
{...}: {
  flake.modules.nixos.beszel-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.beszel = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "beszel hub service";
              enableSingleUserSystem = mkOption {
                type = types.bool;
                default = true;
                description = "Disable password auth for single-user + tinyauth setup (recommended)";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "beszel";
              auth.publicPaths = [
                "^/api"
              ];
            };
        };
        default = {};
        description = "Beszel hub service configuration";
      };
    };
}
