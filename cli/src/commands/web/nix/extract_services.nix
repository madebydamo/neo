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

  names =
    if cfg != null
    then builtins.attrNames (f.nixosConfigurations.${cfg}.config.neo.services or {})
    else [];

  raw =
    if cfg != null
    then
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
    else [];

  ranked = builtins.filter (s: s.rank != null) raw;
  unranked = builtins.filter (s: s.rank == null) raw;

  sortedRanked = builtins.sort (a: b: a.rank < b.rank) ranked;
  sortedUnranked = builtins.sort (a: b: a.name < b.name) unranked;
in
  sortedRanked ++ sortedUnranked
