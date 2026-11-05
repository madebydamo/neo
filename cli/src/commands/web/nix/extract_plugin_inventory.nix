# Infer which neo.services.* options were declared by which *applied* plugin instance.
# Instances are the inputs plugin-<id> (legacy: pluginN, id = input name). Pair each
# with that input's flake.lock original URL — never with the live URL in
# config.neo.core.plugins (settings can change on save before write-flake).
# Instance-aware plugins tag declarations with neo-plugin:<id>#… (lib.neo.plugin);
# older ones are matched by their store path.
{neoFlake}: let
  inherit (import ./extract_lib.nix {inherit neoFlake;}) f cfg tryOr;

  startsWith = prefix: s:
    prefix
    != ""
    && builtins.substring 0 (builtins.stringLength prefix) (toString s) == prefix;

  flakeInputs = f.inputs or {};

  flakeNixText = let
    p = (toString (f.outPath or "")) + "/flake.nix";
  in
    if p != "/flake.nix" && builtins.pathExists p
    then tryOr "" (builtins.readFile p)
    else "";

  urlFromFlakeNix = name: let
    parts = builtins.split ''"?${name}"?[[:space:]]*=[[:space:]]*\{[[:space:]]*url[[:space:]]*=[[:space:]]*"([^"]+)"'' flakeNixText;
    cap =
      if builtins.length parts >= 2 && builtins.isList (builtins.elemAt parts 1)
      then builtins.head (builtins.elemAt parts 1)
      else "";
  in
    if builtins.isString cap
    then cap
    else "";

  lock = let
    p = (toString (f.outPath or "")) + "/flake.lock";
    raw =
      if p != "/flake.lock" && builtins.pathExists p
      then tryOr {} (builtins.fromJSON (builtins.readFile p))
      else {};
  in
    if builtins.isAttrs raw
    then raw
    else {};

  rootInputs = (lock.nodes or {}).root.inputs or {};

  originalToUrl = orig: let
    t = orig.type or "";
    url = orig.url or "";
    owner = orig.owner or "";
    repo = orig.repo or "";
    ref = orig.ref or "";
    dir = orig.dir or "";
    qs =
      if dir != ""
      then "?dir=${dir}"
      else "";
    refSuf =
      if ref != ""
      then "/${ref}"
      else "";
    stripLeadingSlashes = rest: let
      m = builtins.match "/+(.*)" rest;
    in
      if m == null
      then rest
      else builtins.head m;
    gitFile = rawUrl: let
      rest =
        if startsWith "file://" rawUrl
        then builtins.substring 7 (builtins.stringLength rawUrl) rawUrl
        else if startsWith "file:" rawUrl
        then builtins.substring 5 (builtins.stringLength rawUrl) rawUrl
        else rawUrl;
    in "git+file:/${stripLeadingSlashes rest}";
  in
    if !(builtins.isAttrs orig)
    then ""
    else if (t == "github" || t == "gitlab" || t == "sourcehut") && owner != "" && repo != ""
    then "${t}:${owner}/${repo}${refSuf}${qs}"
    else if t == "path"
    then let
      p = orig.path or url;
    in
      if p == ""
      then ""
      else if startsWith "path:" p
      then p
      else "path:${p}"
    else if t == "git" && url != ""
    then
      if startsWith "file:" url || startsWith "file://" url
      then gitFile url
      else if startsWith "git+" url
      then url
      else if startsWith "http://" url || startsWith "https://" url
      then "git+${url}"
      else url
    else url;

  urlForInput = name: let
    fromNix = urlFromFlakeNix name;
    nodeId = rootInputs.${name} or name;
    nodeName =
      if builtins.isString nodeId
      then nodeId
      else name;
    orig = tryOr {} ((lock.nodes or {}).${nodeName}.original or {});
    fromLock = originalToUrl orig;
  in
    if fromNix != ""
    then fromNix
    else fromLock;

  # plugin-<id> (instances) and pluginN (configs from before instances).
  idOf = name: let
    m = builtins.match "plugin-(.+)" name;
  in
    if m != null
    then builtins.head m
    else if builtins.match "plugin[0-9]+" name != null
    then name
    else null;

  pluginNames = builtins.sort (a: b: a < b) (
    builtins.filter (n: idOf n != null) (builtins.attrNames flakeInputs)
  );

  livePlugins =
    if cfg == null
    then {}
    else tryOr {} (f.nixosConfigurations.${cfg}.config.neo.core.plugins or {});

  pluginAt = name: let
    inp = flakeInputs.${name} or null;
    id = idOf name;
    outPath =
      if inp == null
      then ""
      else toString (tryOr "" (inp.outPath or ""));
  in {
    inherit id outPath;
    url = urlForInput name;
    prefix = (livePlugins.${id} or {}).prefix or "";
  };

  pluginList = map pluginAt pluginNames;

  serviceNames =
    if cfg == null
    then []
    else builtins.attrNames (f.nixosConfigurations.${cfg}.options.neo.services or {});

  declsOf = name: let
    raw = tryOr [] (
      f.nixosConfigurations.${cfg}.options.neo.services.${name}.declarations or []
    );
  in
    map toString (
      if builtins.isList raw
      then raw
      else []
    );

  ownersOf = name: let
    decls = declsOf name;
    byTag = builtins.filter (p: builtins.any (d: startsWith "neo-plugin:${p.id}#" d) decls) pluginList;
    byPath =
      builtins.filter (
        p:
          p.outPath
          != ""
          && builtins.any (d: startsWith p.outPath d) decls
      )
      pluginList;
  in
    map (p: p.id) (
      if byTag != []
      then byTag
      else byPath
    );

  owners = builtins.listToAttrs (
    builtins.filter (x: x.value != []) (
      map (n: {
        name = n;
        value = ownersOf n;
      })
      serviceNames
    )
  );

  plugins =
    map (p: {
      inherit (p) id url prefix;
      services = builtins.filter (n: builtins.elem p.id (owners.${n} or [])) serviceNames;
    })
    pluginList;
in {
  inherit owners plugins;
}
