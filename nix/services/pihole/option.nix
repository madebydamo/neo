# Pi-hole service options.
{...}: {
  flake.modules.nixos.pihole-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.pihole = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "pihole ad-blocking DNS service";
              webPassword = mkOption {
                type = types.str;
                default = "";
                description = "Password for Pi-hole web admin interface";
              };
              upstream = mkOption {
                type = types.str;
                default = "9.9.9.9;1.1.1.1";
                description = "Semicolon separated list of upstream dns servers";
              };
              localIP = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Local IP address to forward services.swag.domain toward to.";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "pihole";
              auth.enabled = true;
            };
        };
        default = {};
        description = "Pi-hole service configuration";
      };
    };
}
