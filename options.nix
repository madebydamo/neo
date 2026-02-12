{
  config,
  lib,
  ...
}:
with lib; {
  options.neo.volumes = mkOption {
    type = types.attrsOf types.str;
    default = {};
    description = lib.mdDoc "Volume mappings from host to container";
  };

  options.neo.uid = mkOption {
    type = types.int;
    default = 1000;
    description = lib.mdDoc "Global UID for services and containers";
  };

  options.neo.gid = mkOption {
    type = types.int;
    default = 1000;
    description = lib.mdDoc "Global GID for services and containers";
  };

  options.neo.ssh.authorizedKeys = mkOption {
    type = types.listOf types.str;
    default = [];
    description = lib.mdDoc "SSH authorized keys for root in VM";
  };
}
