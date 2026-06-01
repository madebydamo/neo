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
    else if cfgNames != []
    then builtins.head cfgNames
    else null;

  services =
    if cfg != null
    then (f.nixosConfigurations.${cfg}.config.neo.services or {})
    else {};
  swag = services.swag or {};
  domain = swag.domain or null;

  isProxied = n: let
    v = services.${n} or {};
  in
    n != "swag" && (v.enabled or false) && (v.subdomain or null) != null;

  proxiedNames = builtins.filter isProxied (builtins.attrNames services);
in {
  domain = domain;
  services =
    map (n: let
      svc = services.${n};
      meta = svc.meta or {};
    in {
      name = n;
      subdomain = svc.subdomain or "";
      icon = meta.icon or null;
    })
    proxiedNames;
}
