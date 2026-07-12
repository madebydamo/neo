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

  neoConfig =
    if cfg != null
    then (f.nixosConfigurations.${cfg}.config.neo or {})
    else {};
  services = neoConfig.services or {};
  swag = services.swag or {};
  domain = swag.domain or null;
  hostname = neoConfig.core.hostname or "nixos";

  isProxied = n: let
    v = services.${n} or {};
  in
    n != "swag" && (v.enabled or false) && (v.subdomain or null) != null;

  proxiedNames = builtins.filter isProxied (builtins.attrNames services);

  raw =
    map (n: let
      svc = services.${n};
      meta = svc.meta or {};
    in {
      name = n;
      subdomain = svc.subdomain or "";
      icon = meta.icon or null;
      rank = meta.rank or null;
      iframeCompatible = meta.iframeCompatible or true;
    })
    proxiedNames;

  ranked = builtins.filter (s: s.rank != null) raw;
  unranked = builtins.filter (s: s.rank == null) raw;

  sortedRanked = builtins.sort (a: b: a.rank < b.rank) ranked;
  sortedUnranked = builtins.sort (a: b: a.name < b.name) unranked;
  theme = let
    t = (services.neo or {}).theme or "lofi";
  in
    if builtins.isString t
    then t
    else "lofi";
in {
  domain = domain;
  hostname = hostname;
  theme = theme;
  services = sortedRanked ++ sortedUnranked;
}
