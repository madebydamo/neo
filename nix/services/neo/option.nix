# Neo web UI service options.
# Exposes the `neo web` config editor at the `neo.*` subdomain.
# Auth enabled by default via mkReverseProxyOptions.
{...}: {
  flake.modules.nixos.neo-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.neo = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption (lib.mdDoc "Neo web UI service (config editor launched via `neo web`)");

              port = mkOption {
                type = types.port;
                internal = true;
                default = 8091;
                description = lib.mdDoc "Internal port for the Neo web UI (binds to 0.0.0.0 on host; proxied via SWAG)";
              };
            }
            // lib.neo.mkReverseProxyOptions {subdomain = "neo";};
        };
        default = {};
        description = lib.mdDoc "Neo web UI configuration";
      };
    };
}
