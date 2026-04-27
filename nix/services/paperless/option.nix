# Paperless service options.
{...}: {
  flake.modules.nixos.paperless-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.paperless = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption (lib.mdDoc "paperless document management service");
              port = mkOption {
                type = types.port;
                default = 8000;
                description = lib.mdDoc "Internal port for paperless web UI";
              };
              dbPassword = mkOption {
                type = types.str;
                default = "your_strong_password_here";
                description = lib.mdDoc "Password for internal docker connection";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "paperless";
            };
        };
        default = {};
        description = lib.mdDoc "Paperless service configuration";
      };
    };
}
