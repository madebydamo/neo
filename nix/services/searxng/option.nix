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
            }
            // neo.mkVpnOptions {
              containers = ["searxng"];
              internalContainers = ["searxng-redis"];
              networks = ["internal"];
              ports = [8080];
            };
        };
        default = {};
        description = "Searxng service configuration";
      };
    };
}
