# Every setting's path and label, for the chat drawer's `@` references
# (routes/agent.rs → /agent/refs). Cheap on purpose: no types, defaults or
# current values (those come from extract_service_options.nix per service when
# a reference is sent). Paths match the option pane's flattened names:
#   { services = { nextcloud = [ { path = "subdomain"; label = null; secret = false; group = "access"; } … ]; };
#     core = [ { path = "timeZone"; … } ]; }
# Walk rules mirror extract_service_options.nix: plain submodules expand to
# dotted children, listOf / attrsOf / widget submodules stay one field,
# internal options and `meta` are skipped.
{neoFlake}: let
  inherit (import ./extract_lib.nix {inherit neoFlake;}) f cfg tryOr safeList;
  neoOptions = tryOr {} (f.nixosConfigurations.${cfg}.options.neo or {});

  isOption = o: builtins.isAttrs o && (o._type or null) == "option";
  pubNames = o:
    builtins.filter (
      k: k != "_freeformOptions" && builtins.substring 0 1 k != "_"
    ) (builtins.attrNames o);

  groupId = ui: let
    g = tryOr null (ui.group or null);
  in
    if builtins.isAttrs g
    then tryOr null (g.id or null)
    else null;

  str = v:
    if builtins.isString v && v != ""
    then v
    else null;

  walk = group: pathList: o:
    if isOption o
    then let
      internal = tryOr false (o.internal or false);
      ui = tryOr {} (o.ui or {});
      ownGroup = groupId ui;
      g =
        if ownGroup != null
        then ownGroup
        else group;
      t = tryOr {} (o.type or {});
      tn = tryOr "" (t.name or "");
      subSet = tryOr {} (
        if builtins.isAttrs t && t ? getSubOptions
        then t.getSubOptions pathList
        else {}
      );
      widget = tryOr "" (ui.widget or "");
      expand =
        !(builtins.elem tn ["listOf" "attrsOf" "lazyAttrsOf"])
        && subSet != {}
        && widget == "";
    in
      if internal
      then []
      else if expand
      then walk g pathList subSet
      else [
        {
          path = builtins.concatStringsSep "." pathList;
          label = tryOr null (str (ui.label or null));
          secret = tryOr false (o.secret or false);
          group = g;
        }
      ]
    else if builtins.isAttrs o
    then builtins.concatMap (k: walk group (pathList ++ [k]) o.${k}) (pubNames o)
    else [];

  notMeta = r: r.path != "" && builtins.match "^meta(\\..*)?$" r.path == null;
  # One broken service must not empty the whole index.
  index = o: builtins.filter notMeta (safeList (walk null [] o));
in {
  services = builtins.mapAttrs (_: index) (tryOr {} (neoOptions.services or {}));
  core = index (neoOptions.core or {});
}
