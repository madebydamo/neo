{neoFlake}: let
  inherit (import ./extract_lib.nix {inherit neoFlake;}) neoConfig safeList sortByRank guidesEnabled;
  inventory = import ./extract_plugin_inventory.nix {inherit neoFlake;};

  configServices = neoConfig.services or {};
  names = builtins.attrNames configServices;

  # Deprecated services stay listed only while they are enabled.
  raw = builtins.filter (s: s.enabled || !s.deprecated) (
    map (n: let
      svc = configServices.${n};
      meta = svc.meta or {};
      enabled = svc.enabled or false;
    in {
      name = n;
      inherit enabled;
      # Runtime units for the grid status dot (installed services only).
      units =
        if enabled
        then safeList (svc.systemdUnits or [])
        else [];
      timers =
        if enabled
        then safeList (svc.systemdTimers or [])
        else [];
      # A path stays a path. builtins.toJSON copies it to /nix/store/…-file,
      # and the web UI rewrites that to /service-icons/…. A URL string is unchanged.
      icon = meta.icon or null;
      rank = meta.rank or null;
      category =
        if (meta.category or "") != ""
        then meta.category
        else "Other";
      description = meta.description or "";
      # Grid card line. Plugins without a tagline fall back to the first
      # line of their description.
      tagline = let
        t = meta.tagline or "";
        first = builtins.match "[[:space:]]*([^\n]*).*" (meta.description or "");
      in
        if t != ""
        then t
        else if first == null
        then ""
        else builtins.head first;
      replaces = safeList (meta.replaces or []);
      # { spdx, url, openSource, note } or null; the card flags openSource = false.
      license = meta.license or null;
      deprecated = (meta.deprecated or null) != null;
      pluginIds = inventory.owners.${n} or [];
    })
    names
  );

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
  pluginInventory = inventory.plugins;
  inherit guidesEnabled;
}
