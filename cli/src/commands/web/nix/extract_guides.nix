# Guides for the web UI: metadata from neo.guides, the Markdown body with
# {{domain}} / {{service.<name>.url}} filled in, and every service:, option:,
# guide: and privacy: link checked against this configuration (broken ones
# render as plain text).
{
  neoFlake,
  configName ? null,
}: let
  inherit (import ./extract_lib.nix {inherit neoFlake configName;}) f cfg neoConfig tryOr guidesEnabled;

  nixos =
    if cfg != null
    then f.nixosConfigurations.${cfg}
    else {
      config = {};
      options = {};
    };
  services = neoConfig.services or {};
  serviceNames = builtins.attrNames services;
  guidesCfg = tryOr {} (neoConfig.guides or {});
  guideIds = builtins.attrNames guidesCfg;
  domain = tryOr null (services.swag.domain or null);

  serviceUrl = name: let
    sub = tryOr null (services.${name}.subdomain or null);
  in
    if sub == null || domain == null
    then null
    else "https://${sub}.${domain}";

  # Walk option paths below neo.services.<svc> or neo.core (submodules and
  # plain nested option sets). attrsOf entries are not addressable.
  isOption = v: builtins.isAttrs v && (v._type or null) == "option";
  subOptions = v:
    if isOption v
    then tryOr {} (v.type.getSubOptions [])
    else v;
  walk = node: segs:
    if segs == []
    then true
    else let
      opts = subOptions node;
      seg = builtins.head segs;
    in
      builtins.isAttrs opts && opts ? ${seg} && walk opts.${seg} (builtins.tail segs);
  optionExists = path: let
    segs = builtins.filter builtins.isString (builtins.split "\\." path);
    root = builtins.head segs;
    rest = builtins.tail segs;
    neoOpts = tryOr {} (nixos.options.neo or {});
  in
    if root == "core"
    then tryOr false (walk (neoOpts.core or {}) rest)
    else
      builtins.elem root serviceNames
      && tryOr false (walk (neoOpts.services.${root} or {}) rest);

  privacyIds = tryOr [] (builtins.attrNames (neoConfig.privacy.categories or {}));

  linkOk = scheme: target:
    if scheme == "service"
    then builtins.elem target serviceNames
    else if scheme == "option"
    then optionExists target
    else if scheme == "guide"
    then builtins.elem target guideIds
    else if scheme == "privacy"
    then builtins.elem target privacyIds
    else true;

  # [text](scheme:target) links in a Markdown body.
  linksIn = text:
    map (m: {
      scheme = builtins.elemAt m 0;
      target = builtins.elemAt m 1;
    }) (builtins.filter builtins.isList
      (builtins.split "[]][(](service|option|guide|privacy):([^) ]+)[)]" text));

  uniq = builtins.foldl' (acc: x:
    if builtins.elem x acc
    then acc
    else acc ++ [x]) [];

  fill = text: let
    urlNames = builtins.filter (n: serviceUrl n != null) serviceNames;
  in
    builtins.replaceStrings
    (["{{domain}}"] ++ map (n: "{{service.${n}.url}}") urlNames)
    ([
        (
          if domain == null
          then "<your domain>"
          else domain
        )
      ]
      ++ map serviceUrl urlNames)
    text;

  serviceCard = name: let
    svc = services.${name} or {};
    meta = tryOr {} (svc.meta or {});
  in {
    inherit name;
    exists = builtins.elem name serviceNames;
    enabled = tryOr false (svc.enabled or false);
    icon = meta.icon or null;
    tagline = meta.tagline or "";
    url = serviceUrl name;
  };

  guide = id: let
    g = guidesCfg.${id};
    raw = tryOr "" (builtins.readFile g.body);
    links = linksIn raw;
    broken = uniq (map (l: "${l.scheme}:${l.target}") (builtins.filter (l: !(linkOk l.scheme l.target)) links));
    linked = scheme: uniq (map (l: l.target) (builtins.filter (l: l.scheme == scheme) links));
    firstIcon = let
      icons = builtins.filter (i: i != null) (map (s: (serviceCard s).icon) g.services);
    in
      if icons == []
      then null
      else builtins.head icons;
  in {
    inherit id;
    inherit (g) title summary difficulty minutes tags rank privacy;
    icon =
      if g.icon != null
      then g.icon
      else firstIcon;
    services = map serviceCard g.services;
    optional = map serviceCard g.optional;
    # Every service the guide touches: required, optional and linked in the text.
    mentions = uniq (g.services ++ g.optional ++ linked "service" ++ map (o: builtins.head (builtins.filter builtins.isString (builtins.split "\\." o))) (linked "option"));
    guideLinks = linked "guide";
    brokenLinks = broken;
    body = fill raw;
  };

  all = builtins.filter (g: g != null) (map (id: tryOr null (builtins.deepSeq (guide id) (guide id))) guideIds);
  ranked = builtins.sort (a: b: a.rank < b.rank) (builtins.filter (g: g.rank != null) all);
  unranked = builtins.sort (a: b: a.title < b.title) (builtins.filter (g: g.rank == null) all);
in {
  guides =
    if guidesEnabled
    then ranked ++ unranked
    else [];
  enabled = guidesEnabled;
  hermesEnabled = tryOr false (services.hermes.enabled or false);
  inherit domain;
}
