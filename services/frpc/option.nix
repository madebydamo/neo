{ config, lib, ... }:

with lib;

{
  options.neo.services.frpc = mkOption {
    type = types.submodule {
      options = {
        enabled = mkEnableOption (lib.mdDoc "frpc service");
        serverAddr = mkOption {
          type = types.str;
          default = "151.241.217.226";
          description = lib.mdDoc "FRP server address";
        };
        serverPort = mkOption {
          type = types.int;
          default = 7000;
          description = lib.mdDoc "FRP server port";
        };
        token = mkOption {
          type = types.str;
          default = "hbIbv8ljuVLqcNPIsowoxx3jZ38AjUjHT5Fvf3QfIpU=";
          description = lib.mdDoc "FRP authentication token";
        };
        certPath = mkOption {
          type = types.str;
          default = "/path/to/your/filebrowser.damianmoser.ch.crt";
          description = lib.mdDoc "Path to SSL certificate";
        };
        keyPath = mkOption {
          type = types.str;
          default = "/path/to/your/filebrowser.damianmoser.ch.key";
          description = lib.mdDoc "Path to SSL private key";
        };
      };
    };
    default = { };
    description = lib.mdDoc "FRPC service configuration";
  };
}