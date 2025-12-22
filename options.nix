{ config, lib, ... }:
with lib;

{
  options.neo.volumes = mkOption {
    type = types.attrsOf types.str;
    default = { };
    description = lib.mdDoc "Volume mappings from host to container";
  };

}