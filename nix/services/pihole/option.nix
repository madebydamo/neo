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
              enabled = mkEnableOption (lib.mdDoc "pihole ad-blocking DNS service");
              webPassword = mkOption {
                type = types.str;
                default = "";
                description = lib.mdDoc "Password for Pi-hole web admin interface";
              };
              upstream = mkOption {
                type = types.str;
                default = "9.9.9.9;1.1.1.1";
                description = lib.mdDoc "Semicolon separated list of upstream dns servers";
              };
              localIP = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = lib.mdDoc "Local IP address to forward services.swag.domain toward to.";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "pihole";
              auth.enabled = true;
            };
        };
        default = {};
        description = lib.mdDoc "Pi-hole service configuration";
      };
    };
}
