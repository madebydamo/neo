# Example service options.
{...}: {
  flake.modules.nixos.example-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.example = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Example service";
            }
            // neo.mkReverseProxyOptions {
              subdomain = "example";
              auth.publicPaths = [
                "^/share/"
                "^/static/"
                "^/api/public"
              ];
            };
        };
        default = {};
        description = "Example service configuration";
      };
    };
}
