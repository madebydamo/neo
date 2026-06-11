{
  neoFlake,
  configName ? null,
}: let
  f =
    if builtins.isString neoFlake
    then builtins.getFlake neoFlake
    else neoFlake;
  cfgNames = builtins.attrNames (f.nixosConfigurations or {});
  cfg =
    if configName != null
    then configName
    else if builtins.elem "homeserver" cfgNames
    then "homeserver"
    else if builtins.elem "neo" cfgNames
    then "neo"
    else if cfgNames != []
    then builtins.head cfgNames
    else null;
  neoSvc =
    if cfg != null
    then (f.nixosConfigurations.${cfg}.config.neo.services.neo or {})
    else {};
  theme = neoSvc.theme or "lofi";
in
  if builtins.isString theme
  then theme
  else "lofi"
