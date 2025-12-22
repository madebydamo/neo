{ config, lib, ... }:
with lib;

{
  options.neo.volumes = mkOption {
    type = types.attrsOf types.str;
    default = { };
    description = lib.mdDoc "Volume mappings from host to container";
  };

  options.neo.services = mkOption {
    type = types.attrs;
    default = { };
    description = lib.mdDoc "Services configuration";
  };

}