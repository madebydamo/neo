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

              iframeCookieSupport = mkOption {
                type = types.bool;
                default = true;
                description = lib.mdDoc ''
                  When enabled (default), automatically writes a global nginx configuration file
                  into SWAG's conf.d that relaxes SameSite cookies (SameSite=None + Secure) and
                  sets a broad Domain (based on the main domain). This makes authenticated services
                  work correctly when loaded inside the neo web UI iframes.
                  Set to false to opt out completely.
                '';
              };
            }
            // lib.neo.mkReverseProxyOptions {subdomain = "neo";};
        };
        default = {};
        description = lib.mdDoc "Neo web UI configuration";
      };
    };
}
