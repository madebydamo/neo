{
  config,
  lib,
  ...
}:
with lib; {
  options.neo.services.rathole = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (lib.mdDoc "rathole client service");
        token = mkOption {
          type = types.str;
          description = lib.mdDoc "Authentication token for rathole";
        };
        remoteAddr = mkOption {
          type = types.str;
          description = lib.mdDoc "Remote server address for rathole";
        };
        port = mkOption {
          type = types.port;
          default = 2333;
          description = lib.mdDoc "Remote server port for rathole";
        };
        name = mkOption {
          type = types.str;
          description = lib.mdDoc "Name prefix for rathole services";
        };
      };
    };
    default = {};
    description = lib.mdDoc "Rathole client configuration";
  };
}
