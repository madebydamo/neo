# Vaultwarden service options.
{...}: {
  flake.modules.nixos.vaultwarden-option = {lib, ...}:
    with lib; {
      options.neo.services.vaultwarden = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "vaultwarden password manager service";
              port = mkOption {
                type = types.port;
                default = 8888;
                description = "Internal port vaultwarden listens on (ROCKET_PORT)";
              };
              adminToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Random auth token to authenticate in admin page";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "vaultwarden";
              auth.publicPaths = [
                "^/api/"
                "^/identity/"
                "^/notifications/"
                "^/icons/"
              ];
            };
        };
        default = {};
        description = "Vaultwarden service configuration";
      };
    };
}
