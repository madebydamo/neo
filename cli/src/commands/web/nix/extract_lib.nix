# Shared helpers for the extract_*.nix evaluators (written next to them).
{
  neoFlake,
  configName ? null,
}: rec {
  f =
    if builtins.isString neoFlake
    then builtins.getFlake neoFlake
    else neoFlake;

  # nixosConfigurations entry to read: explicit, else homeserver, neo, or the first one.
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

  # config.neo of the selected configuration.
  neoConfig =
    if cfg != null
    then (f.nixosConfigurations.${cfg}.config.neo or {})
    else {};

  tryOr = def: x: let
    r = builtins.tryEval x;
  in
    if r.success
    then r.value
    else def;

  # A broken unit list on one service must not take down a whole page.
  safeList = v: let
    r = builtins.tryEval (builtins.deepSeq v v);
  in
    if r.success && builtins.isList r.value
    then r.value
    else [];

  # Ranked first (asc), then unranked by name.
  sortByRank = services: let
    ranked = builtins.filter (s: s.rank != null) services;
    unranked = builtins.filter (s: s.rank == null) services;
  in
    builtins.sort (a: b: a.rank < b.rank) ranked
    ++ builtins.sort (a: b: a.name < b.name) unranked;

  # Guides tab + privacy map (neo.services.neo.guides, on by default).
  guidesEnabled = let
    g = (neoConfig.services or {}).neo.guides or true;
  in
    !(builtins.isBool g) || g;

  # DaisyUI theme from neo.services.neo.theme (light or dark, cli/web-css/input.css).
  theme = let
    t = (neoConfig.services or {}).neo.theme or "light";
  in
    if t == "dark"
    then "dark"
    else "light";
}
