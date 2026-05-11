{neoFlake}: let
  f =
    if builtins.isString neoFlake
    then builtins.getFlake neoFlake
    else neoFlake;

  cfgNames = builtins.attrNames (f.nixosConfigurations or {});
  cfg =
    if builtins.elem "homeserver" cfgNames
    then "homeserver"
    else if builtins.elem "neo" cfgNames
    then "neo"
    else builtins.head cfgNames;

  names = builtins.attrNames (f.nixosConfigurations.${cfg}.config.neo.services or {});
in
  map (n: {
    name = n;
    enabled = f.nixosConfigurations.${cfg}.config.neo.services.${n}.enabled or false;
  })
  names
