# Pastebin (wantguns/bin) service options.
# Tinyauth forward auth is disabled by default.
{...}: {
  flake.modules.nixos.pastebin-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.pastebin = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption ("pastebin (wantguns/bin) service");
              port = mkOption {
                type = types.port;
                default = 6163;
                description = "Internal port the pastebin service listens on";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "pastebin";
              auth.enabled = false;
            };
        };
        default = {};
        description = "Pastebin service configuration";
      };
    };
}
