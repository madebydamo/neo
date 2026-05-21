# Searxng service options.
{...}: {
  flake.modules.nixos.searxng-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.searxng = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "searxng service";
            }
            // neo.mkReverseProxyOptions {
              subdomain = "search";
              auth = {
                enabled = false;
              };
            };
        };
        default = {};
        description = "Searxng service configuration";
      };
    };
}

