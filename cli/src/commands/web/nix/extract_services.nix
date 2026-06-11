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
  map (n: let
    svc = f.nixosConfigurations.${cfg}.config.neo.services.${n} or {};
    meta = svc.meta or {};
  in {
    name = n;
    enabled = svc.enabled or false;
    icon = meta.icon or null;
    rank = meta.rank or null;
  })
  names
