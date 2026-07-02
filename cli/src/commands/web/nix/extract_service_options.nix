{
  neoFlake,
  service ? null,
  section ? null,
  configName ? null,
}: let
  f =
    if builtins.isString neoFlake
    then builtins.getFlake neoFlake
    else neoFlake;
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
  servicesOpt = f.nixosConfigurations.${cfg}.options.neo.services or {};
  coreSections = ["ssh" "volumes" "timeZone" "uid" "gid" "hostname" "hashedLinuxPassword"];
  # These are the leaf sections that live under neo.core.* (for individual panes or dotted names in aggregate "core" pane).
  # The aggregate "core" itself is looked up as neo.core (falls to the else branch).
  getNeoOpt = s:
    if builtins.elem s coreSections
    then f.nixosConfigurations.${cfg}.options.neo.core.${s} or {}
    else f.nixosConfigurations.${cfg}.options.neo.${s} or {};
  getNeoConf = s:
    if builtins.elem s coreSections
    then f.nixosConfigurations.${cfg}.config.neo.core.${s} or {}
    else f.nixosConfigurations.${cfg}.config.neo.${s} or {};
  root =
    if service != null
    then servicesOpt.${service} or {}
    else if section != null
    then getNeoOpt section
    else {};
  configServices = f.nixosConfigurations.${cfg}.config.neo.services or {};
  configRoot =
    if service != null
    then configServices.${service} or {}
    else if section != null
    then getNeoConf section
    else {};

  tryOr = def: x: let
    r = builtins.tryEval x;
  in
    if r.success
    then r.value
    else def;

  getNested = pathList: attrs:
    if pathList == []
    then attrs
    else if builtins.isAttrs attrs && builtins.hasAttr (builtins.head pathList) attrs
    then getNested (builtins.tail pathList) attrs.${builtins.head pathList}
    else null;

  isSafeJson = v: let
    t = builtins.typeOf v;
  in
    t == "null" || t == "bool" || t == "int" || t == "float" || t == "string" || t == "list" || t == "set";

  toSafeValue = v: let
    r = builtins.tryEval v;
    val =
      if r.success
      then r.value
      else null;
    t = builtins.typeOf val;
  in
    if t == "null" || t == "bool" || t == "int" || t == "float" || t == "string"
    then val
    else if t == "list" || t == "set"
    then
      if
        builtins.all isSafeJson (
          if t == "list"
          then val
          else builtins.attrValues val
        )
      then val
      else builtins.toJSON val
    else builtins.toJSON val;

  getTypeInfo = t: let
    n0 = tryOr "" (
      if builtins.hasAttr "name" t
      then t.name
      else ""
    );
    rawName =
      if n0 == "string"
      then "str"
      else n0;
    n =
      if rawName == "unsignedInt16"
      then "port"
      else if
        rawName
        == "intBetween"
        || rawName == "unsignedInt"
        || rawName == "positiveInt"
        || rawName == "signedInt8"
        || rawName == "signedInt16"
        || rawName == "signedInt32"
        || rawName == "unsignedInt8"
        || rawName == "unsignedInt32"
      then "int"
      else if rawName == "numberBetween" || rawName == "numberNonnegative"
      then "float"
      else rawName;
    nested = tryOr {} (t.nestedTypes or {});
    elemT = tryOr null (nested.elemType or null);
    elemInfo =
      if builtins.isAttrs elemT
      then getTypeInfo elemT
      else null;
    functor = tryOr {} (t.functor or {});
    fName = tryOr "" (functor.name or "");
    fPayload = tryOr null (functor.payload or null);
    desc = tryOr "" (t.description or "");
    bounds = let
      base =
        if fName == "between" && builtins.isAttrs fPayload
        then {
          min = fPayload.lo or null;
          max = fPayload.hi or null;
        }
        else {};
      m = builtins.match ".*between ([0-9]+) and ([0-9]+) \\(both inclusive\\).*" desc;
      m2 = builtins.match ".*between ([0-9]+) and ([0-9]+).*" desc;
      fromM = ms:
        if builtins.isList ms && builtins.length ms >= 2
        then {
          min = builtins.fromJSON (builtins.elemAt ms 0);
          max = builtins.fromJSON (builtins.elemAt ms 1);
        }
        else {};
      bDesc =
        if builtins.isList m && builtins.length m >= 2
        then fromM m
        else if builtins.isList m2 && builtins.length m2 >= 2
        then fromM m2
        else {};
      bSpecial =
        if
          builtins.match ".*unsigned integer, meaning >=0.*" desc
          != null
          || builtins.match ".*nonnegative.*" desc != null
        then {min = 0;}
        else if builtins.match ".*positive integer, meaning >0.*" desc != null
        then {min = 1;}
        else {};
    in
      if bDesc != {}
      then bDesc
      else if bSpecial != {}
      then bSpecial
      else base;
    enumVals =
      if fName == "enum"
      then
        if builtins.isList fPayload
        then fPayload
        else if builtins.isAttrs fPayload && builtins.hasAttr "values" fPayload && builtins.isList (fPayload.values or null)
        then fPayload.values
        else null
      else null;
  in
    if n == "nullOr"
    then {
      kind = "nullOr";
      elem = elemInfo;
    }
    else if n == "listOf"
    then {
      kind = "listOf";
      elem = elemInfo;
    }
    else if n == "attrsOf" || n == "lazyAttrsOf"
    then {
      kind = "attrsOf";
      elem = elemInfo;
    }
    else if n == "submodule"
    then {kind = "submodule";}
    else if n == "port"
    then (bounds // {kind = "port";})
    else if n == "int"
    then (bounds // {kind = "int";})
    else if n == "float"
    then (bounds // {kind = "float";})
    else if n == "bool"
    then {kind = "bool";}
    else if n == "str"
    then {kind = "str";}
    else if n == "path"
    then {kind = "path";}
    else if n == "enum"
    then {
      kind = "enum";
      values = enumVals;
    }
    else {
      kind =
        if n == ""
        then "any"
        else n;
    };

  typeLabel = info: let
    k = info.kind or "any";
    e = info.elem or null;
    subL =
      if builtins.isAttrs e
      then typeLabel e
      else "any";
  in
    if k == "nullOr"
    then "null or ${subL}"
    else if k == "listOf"
    then "list of ${subL}"
    else if k == "attrsOf"
    then "attrs of ${subL}"
    else if k == "submodule"
    then "submodule"
    else if k == "port"
    then "port"
    else if k == "int"
    then let
      lo = info.min or null;
      hi = info.max or null;
    in
      if lo != null && hi != null
      then "int (${toString lo}-${toString hi})"
      else if lo != null
      then "int (>=${toString lo})"
      else if hi != null
      then "int (<=${toString hi})"
      else "int"
    else if k == "float"
    then let
      lo = info.min or null;
      hi = info.max or null;
    in
      if lo != null && hi != null
      then "float (${toString lo}-${toString hi})"
      else if lo != null
      then "float (>=${toString lo})"
      else if hi != null
      then "float (<=${toString hi})"
      else "float"
    else if k == "enum"
    then "enum"
    else k;

  mkOptionRecord = path: o: let
    t = tryOr {} (o.type or {});
    ti = getTypeInfo t;
    defVal = tryOr null (
      if builtins.hasAttr "default" o
      then o.default
      else null
    );
    curPath =
      if path == ""
      then []
      else builtins.filter builtins.isString (builtins.split "\\." path);
    curVal = tryOr null (
      if curPath == []
      then null
      else getNested curPath configRoot
    );
    exVal = tryOr null (
      if builtins.hasAttr "example" o
      then o.example
      else null
    );
    descVal = tryOr "" (
      if builtins.hasAttr "description" o
      then o.description
      else ""
    );
    internal = tryOr false (o.internal or false);
    readOnly = tryOr false (o.readOnly or false);
    rank = tryOr null (o.rank or null);
  in {
    name = path;
    type = ti;
    typeLabel = typeLabel ti;
    default = toSafeValue defVal;
    description = descVal;
    example = toSafeValue exVal;
    internal = internal;
    readOnly = readOnly;
    current = toSafeValue curVal;
    rank = rank;
  };

  walk = pathList: o: let
    path =
      if pathList == []
      then ""
      else builtins.concatStringsSep "." pathList;
  in
    if builtins.isAttrs o && builtins.hasAttr "_type" o && o._type == "option"
    then let
      internal = tryOr false (o.internal or false);
      # Completely skip internal options and everything nested under them
      # (e.g. the `meta` submodule we declare via mkServiceMeta).
      # This prevents leaking meta.icon / meta.description etc. into the form fields.
    in
      if internal
      then []
      else let
        record = mkOptionRecord path o;
        t = tryOr null (o.type or null);
        subSet = tryOr {} (
          if builtins.isAttrs t && builtins.hasAttr "getSubOptions" t
          then t.getSubOptions pathList
          else {}
        );
        subs =
          if subSet == {}
          then []
          else let
            tn = tryOr "" (
              if builtins.isAttrs t && builtins.hasAttr "name" t
              then t.name
              else ""
            );
            ph =
              if tn == "listOf"
              then ["*"]
              else if tn == "attrsOf" || tn == "lazyAttrsOf"
              then ["<name>"]
              else [];
          in
            walk (pathList ++ ph) subSet;
      in
        [record] ++ subs
    else if builtins.isAttrs o
    then let
      ns = builtins.attrNames o;
      pub = builtins.filter (k: k != "_freeformOptions" && builtins.substring 0 1 k != "_") ns;
    in
      builtins.concatLists (map (k: walk (pathList ++ [k]) o.${k}) pub)
    else [];

  raw = walk [] root;

  visible =
    builtins.filter (
      r:
        !(r.internal or false)
        && (r.type.kind or null) != "submodule"
        # Belt-and-suspenders: never expose anything under an internal meta block
        && !(builtins.match "^meta(\\..*)?$" (r.name or "") != null)
    )
    raw;

  ranked = builtins.filter (r: (r.rank or null) != null) visible;
  unranked = builtins.filter (r: (r.rank or null) == null) visible;

  sortedRanked = builtins.sort (a: b: (a.rank or 0) < (b.rank or 0)) ranked;

  sortKey = r: let
    n = r.name or "";
  in
    if n == "enabled"
    then "0\u0000" + n
    else if (r.readOnly or false)
    then "2\u0000" + n
    else "1\u0000" + n;

  sortedUnranked = builtins.sort (a: b: (sortKey a) < (sortKey b)) unranked;

  sorted = sortedRanked ++ sortedUnranked;

  meta = tryOr {} (configRoot.meta or {});
  units = tryOr [] (configRoot.systemdUnits or []);
  containers = tryOr {} (configRoot.containers or {});
in {
  meta =
    if meta == {}
    then null
    else meta;
  options = sorted;
  units = units;
  containers = containers;
}
