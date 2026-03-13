# Tinyauth forward authentication service options.
{...}: {
  flake.modules.nixos.tinyauth-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.tinyauth = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption (lib.mdDoc "tinyauth forward authentication service");
              port = mkOption {
                type = types.port;
                default = 3000;
                description = lib.mdDoc "Port on which tinyauth listens";
              };
              users = mkOption {
                type = types.listOf types.str;
                default = [];
                description = lib.mdDoc ''
                  List of users in username:bcrypt_hash format.
                  Generate with: docker run -i -t --rm ghcr.io/steveiliop56/tinyauth:v5 user create --interactive
                '';
              };
              sessionExpiry = mkOption {
                type = types.int;
                default = 86400;
                description = lib.mdDoc "Session expiry time in seconds (default 24h)";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "tinyauth";
              auth.enabled = false;
            };
        };
        default = {};
        description = lib.mdDoc "Tinyauth forward authentication service configuration";
      };
    };
}
