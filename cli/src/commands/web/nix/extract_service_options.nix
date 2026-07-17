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
  coreSections = ["ssh" "volumes" "timeZone" "uid" "gid" "hostname" "hashedLinuxPassword" "plugins"];
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
  # Config is best-effort: option schema must still extract if some paths/defaults
  # are unreadable in the evaluator environment (e.g. local DATA/* perms).
  configServices = tryOr {} (f.nixosConfigurations.${cfg}.config.neo.services or {});
  configRoot = tryOr {} (
    if service != null
    then configServices.${service} or {}
    else if section != null
    then getNeoConf section
    else {}
  );

  tryOr = def: x: let
    r = builtins.tryEval x;
  in
    if r.success
    then r.value
    else def;

  # --- Level-dependent ranking (sibling sort) ---
  # Ranks compete only among siblings at the same nesting level. Groups
  # (submodules like vpn/auth/skill/containers, plain attrsets) stay contiguous.
  # Intermediate attrsets without their own rank inherit min(descendant ranks)
  # so e.g. core.nix.* and core.volumes.* keep a stable placement.
  # Service band table: nix/lib/option.nix.
  isOption = o: builtins.isAttrs o && (o._type or null) == "option";

  pubNames = o:
    builtins.filter (
      k: k != "_freeformOptions" && builtins.substring 0 1 k != "_"
    ) (builtins.attrNames o);

  # Zero-pad ranks so string lexicographic order matches numeric order.
  padRank = n: let
    s = toString n;
    len = builtins.stringLength s;
  in
    if len >= 8
    then s
    else (builtins.substring 0 (8 - len) "00000000") + s;

  optionRank = o:
    if isOption o
    then tryOr null (o.rank or null)
    else null;

  optionReadOnly = o:
    if isOption o
    then tryOr false (o.readOnly or false)
    else false;

  minDescendantRank = o: let
    collect = x:
      if !(builtins.isAttrs x)
      then []
      else if isOption x
      then let
        internal = tryOr false (x.internal or false);
        r = tryOr null (x.rank or null);
      in
        if internal
        then []
        else if r != null
        then [r]
        else []
      else builtins.concatLists (map (k: collect x.${k}) (pubNames x));
    ranks = collect o;
  in
    if ranks == []
    then null
    else
      builtins.foldl' (
        a: b:
          if a < b
          then a
          else b
      ) (builtins.head ranks) (builtins.tail ranks);

  # Options use their own rank (submodule parents place the whole group).
  # Plain attrset groups inherit min ranked descendant.
  siblingRank = o:
    if isOption o
    then optionRank o
    else if builtins.isAttrs o
    then minDescendantRank o
    else null;

  # Band 0 = ranked; band 1 = unranked (enabled, normal, readOnly).
  siblingSortKey = name: child: let
    rank = siblingRank child;
    ro = optionReadOnly child;
  in
    if rank != null
    then "0\u0000" + padRank rank + "\u0000" + name
    else if name == "enabled"
    then "1\u0000" + "0\u0000" + name
    else if ro
    then "1\u0000" + "2\u0000" + name
    else "1\u0000" + "1\u0000" + name;

  sortSiblingNames = attrs:
    builtins.sort (
      a: b: (siblingSortKey a attrs.${a}) < (siblingSortKey b attrs.${b})
    ) (pubNames attrs);

  getNested = pathList: attrs:
    if pathList == []
    then attrs
    else if builtins.isAttrs attrs && builtins.hasAttr (builtins.head pathList) attrs
    then getNested (builtins.tail pathList) attrs.${builtins.head pathList}
    else null;

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
    else if t == "list"
    then map toSafeValue val
    else if t == "set"
    then
      builtins.listToAttrs (
        map (
          k: {
            name = k;
            value = toSafeValue val.${k};
          }
        ) (builtins.attrNames val)
      )
    else null;

  # Serialize UI helper metadata. Path→string ONLY here (not in toSafeValue), so
  # existing types.path option defaults/currents keep serializing as null.
  toHelperMeta = h:
    if !(builtins.isAttrs h)
    then null
    else let
      kind = tryOr "" (h.kind or "");
      scriptRaw = h.script or null;
      script =
        if scriptRaw == null
        then null
        else if builtins.typeOf scriptRaw == "path"
        then toString scriptRaw
        else if builtins.typeOf scriptRaw == "string"
        then scriptRaw
        else null;
      inputs = tryOr [] (h.inputs or []);
    in
      if kind == "" || script == null
      then null
      else {
        id = tryOr "unnamed" (h.id or "unnamed");
        inherit kind;
        label = tryOr "Generate" (h.label or "Generate");
        description = tryOr "" (h.description or "");
        apply = tryOr "set" (h.apply or "set");
        inherit script;
        inputs = map (
          i: {
            name = i.name or "";
            type = i.type or "str";
            label = i.label or (i.name or "");
            required = tryOr true (i.required or true);
            placeholder = tryOr "" (i.placeholder or "");
            default = toSafeValue (i.default or null);
            values = tryOr null (i.values or null);
          }
        ) (builtins.filter (i: builtins.isAttrs i && (i.name or "") != "") inputs);
      };

  typeNameOf = t: let
    n0 = tryOr "" (
      if builtins.isAttrs t && builtins.hasAttr "name" t
      then t.name
      else ""
    );
    rawName =
      if n0 == "string"
      then "str"
      else n0;
  in
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
    else if rawName == "lazyAttrsOf"
    then "attrsOf"
    else rawName;

  callGetSubOptions = t: prefix:
    tryOr {} (
      if builtins.isAttrs t && builtins.hasAttr "getSubOptions" t
      then t.getSubOptions prefix
      else {}
    );

  # Field records for a submodule type (used as attrsOf/listOf element schema).
  # Plain nested submodules are expanded to dotted field names; collections stay as one field.
  # Sibling order matches the main form walk (level-dependent ranks).
  mkFieldRecords = t: let
    walkF = pathList: o:
      if isOption o
      then let
        internal = tryOr false (o.internal or false);
      in
        if internal
        then []
        else let
          path =
            if pathList == []
            then ""
            else builtins.concatStringsSep "." pathList;
          ot = tryOr {} (o.type or {});
          tn = typeNameOf ot;
          ti = getTypeInfo ot;
          record = {
            name = path;
            type = ti;
            typeLabel = typeLabel ti;
            default = toSafeValue (
              if builtins.hasAttr "default" o
              then o.default
              else null
            );
            description = tryOr "" (
              if builtins.hasAttr "description" o
              then o.description
              else ""
            );
            internal = false;
            readOnly = tryOr false (o.readOnly or false);
            current = null;
            rank = tryOr null (o.rank or null);
            helper = toHelperMeta (o.helper or null);
          };
          childSet =
            if tn == "submodule"
            then callGetSubOptions ot pathList
            else {};
        in
          if tn == "submodule" && childSet != {}
          then walkF pathList childSet
          else [record]
      else if builtins.isAttrs o
      then let
        keys = sortSiblingNames o;
      in
        builtins.concatLists (map (k: walkF (pathList ++ [k]) o.${k}) keys)
      else [];
  in
    walkF [] (callGetSubOptions t []);

  getTypeInfo = t: let
    n = typeNameOf t;
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
    else if n == "attrsOf"
    then {
      kind = "attrsOf";
      elem = elemInfo;
    }
    else if n == "submodule"
    then {
      kind = "submodule";
      fields = mkFieldRecords t;
    }
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
    helper = toHelperMeta (o.helper or null);
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
    inherit helper;
  };

  # Walk options into a flat form list, ordered by level-dependent ranks.
  # - Sibling keys are sorted at each nesting level (rank bands, then name).
  # - Plain submodules expand to dotted children (auth.enabled); parent rank
  #   places the whole child block among the parent's siblings.
  # - listOf / attrsOf stay as a single field; element schema lives on type.elem
  #   (including submodule fields). No placeholder paths like entries.<name>.* .
  walk = pathList: o: let
    path =
      if pathList == []
      then ""
      else builtins.concatStringsSep "." pathList;
  in
    if isOption o
    then let
      internal = tryOr false (o.internal or false);
    in
      if internal
      then []
      else let
        t = tryOr null (o.type or null);
        tn = typeNameOf (tryOr {} t);
        subSet = callGetSubOptions (tryOr {} t) pathList;
        expandChildren =
          tn
          != "listOf"
          && tn != "attrsOf"
          && subSet != {};
      in
        # Expanded submodule: emit only children (parent rank used by sibling sort above).
        if expandChildren
        then walk pathList subSet
        else [(mkOptionRecord path o)]
    else if builtins.isAttrs o
    then let
      keys = sortSiblingNames o;
    in
      builtins.concatLists (map (k: walk (pathList ++ [k]) o.${k}) keys)
    else [];

  raw = walk [] root;

  # Walk already emits in hierarchical order; filter only (no global re-sort).
  sorted =
    builtins.filter (
      r:
        !(r.internal or false)
        && (r.type.kind or null) != "submodule"
        # Belt-and-suspenders: never expose anything under an internal meta block
        && !(builtins.match "^meta(\\..*)?$" (r.name or "") != null)
    )
    raw;

  meta = tryOr {} (configRoot.meta or {});
  units = tryOr [] (configRoot.systemdUnits or []);
  containers = tryOr {} (configRoot.containers or {});
  appdata = tryOr null (configRoot.appdata or null);
  appdataRoot = tryOr null (
    f.nixosConfigurations.${cfg}.config.neo.core.volumes.appdata or null
  );
in {
  meta =
    if meta == {}
    then null
    else meta;
  options = sorted;
  units = units;
  containers = containers;
  appdata = appdata;
  appdataRoot = appdataRoot;
}
