{ config, lib, ... }:

{
  imports = [
    ./options.nix
    ./services
  ];

  system.activationScripts.create-neo-volumes = {
    text = "mkdir -p ${builtins.concatStringsSep " " (builtins.attrValues config.neo.volumes)}";
    deps = [ ];
  };

  system.stateVersion = "24.11";
}
