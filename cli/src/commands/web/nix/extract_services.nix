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
        category =
          if (meta.category or "") != ""
          then meta.category
          else "Other";
        description = meta.description or "";
      })
      names
    else [];

  # Preferred category order for the services grid UI.
  categoryOrder = [
    "Core"
    "Network"
    "Security"
    "Media"
    "Files"
    "Monitoring"
    "Utilities"
    "AI"
    "Other"
  ];

  # Ranked first (asc), then unranked by name.
  sortByRank = services: let
    ranked = builtins.filter (s: s.rank != null) services;
    unranked = builtins.filter (s: s.rank == null) services;
    sortedRanked = builtins.sort (a: b: a.rank < b.rank) ranked;
    sortedUnranked = builtins.sort (a: b: a.name < b.name) unranked;
  in
    sortedRanked ++ sortedUnranked;

  # Within a category: installed first, then available; each group by rank/name.
  sortServices = services: let
    enabled = builtins.filter (s: s.enabled) services;
    disabled = builtins.filter (s: !s.enabled) services;
  in
    sortByRank enabled ++ sortByRank disabled;

  # Unique categories present, ordered by categoryOrder then alpha.
  orderedCategories = services: let
    present = builtins.attrNames (
      builtins.listToAttrs (
        map (s: {
          name = s.category;
          value = true;
        })
        services
      )
    );
    known = builtins.filter (c: builtins.elem c present) categoryOrder;
    unknown = builtins.sort (a: b: a < b) (
      builtins.filter (c: !(builtins.elem c categoryOrder)) present
    );
  in
    known ++ unknown;

  groupByCategory = services:
    map (cat: let
      svcs = sortServices (builtins.filter (s: s.category == cat) services);
    in {
      name = cat;
      services = svcs;
      hasEnabled = builtins.any (s: s.enabled) svcs;
      hasDisabled = builtins.any (s: !s.enabled) svcs;
    }) (orderedCategories services);
in {
  groups = groupByCategory raw;
  categories = orderedCategories raw;
}
