# Privacy map: categories (neo.privacy.categories) with the services and
# guides tagged for each, and whether an installed service already keeps that
# data at home.
{
  neoFlake,
  configName ? null,
}: let
  inherit (import ./extract_lib.nix {inherit neoFlake configName;}) neoConfig tryOr guidesEnabled;

  services = neoConfig.services or {};
  cats = tryOr {} (neoConfig.privacy.categories or {});
  guides = tryOr {} (neoConfig.guides or {});

  serviceCard = name: let
    svc = services.${name};
    meta = tryOr {} (svc.meta or {});
  in {
    inherit name;
    enabled = tryOr false (svc.enabled or false);
    icon = meta.icon or null;
    tagline = meta.tagline or "";
    replaces = tryOr [] (meta.replaces or []);
  };

  # Deprecated services are not suggested unless already enabled.
  listed = n:
    tryOr false (services.${n}.enabled or false)
    || tryOr null ((services.${n}.meta or {}).deprecated or null) == null;

  servicesFor = id:
    map serviceCard (builtins.filter (n: listed n && builtins.elem id (tryOr [] ((services.${n}.meta or {}).privacy or [])))
      (builtins.attrNames services));

  guidesFor = id:
    map (gid: {
      id = gid;
      inherit (guides.${gid}) title summary minutes difficulty;
    }) (builtins.filter (gid: builtins.elem id (tryOr [] (guides.${gid}.privacy or []))) (builtins.attrNames guides));

  category = id: let
    c = cats.${id};
    svcs = servicesFor id;
    # Installed first, then by name.
    sorted =
      builtins.filter (s: s.enabled) svcs
      ++ builtins.filter (s: !s.enabled) svcs;
  in {
    inherit id;
    inherit (c) label icon cloud home examples rank;
    services = sorted;
    guides = guidesFor id;
    home_now = builtins.any (s: s.enabled) svcs;
  };

  all = builtins.filter (c: c != null) (map (id: tryOr null (builtins.deepSeq (category id) (category id))) (builtins.attrNames cats));
in {
  enabled = guidesEnabled;
  categories =
    builtins.sort (a: b:
      if a.rank == b.rank
      then a.label < b.label
      else a.rank < b.rank)
    (
      if guidesEnabled
      then all
      else []
    );
}
