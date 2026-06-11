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
              enabled = mkEnableOption "Example service" // {rank = 0;};
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
