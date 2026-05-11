{
  neoFlake,
  service,
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
    else builtins.head cfgNames;
  root = f.nixosConfigurations.${cfg}.options.neo.services.${service} or {};
  configRoot = f.nixosConfigurations.${cfg}.config.neo.services.${service} or {};

  safeString = x:
    if builtins.isString x
    then x
    else if x == null
    then ""
    else builtins.toJSON x;

  getType = t:
    if builtins.hasAttr "name" t
    then
      let n = t.name;
      in
      if n == "nullOr"
      then
        let
          inner = builtins.tryEval (t.nestedTypes.elemType or {});
        in
          if inner.success && builtins.hasAttr "name" inner.value
          then "nullOr ${inner.value.name}"
          else "nullOr"
      else n
    else if builtins.hasAttr "description" t
    then t.description
    else "unknown";

  tryOr = def: x: let
    res = builtins.tryEval x;
  in
    if res.success
    then res.value
    else def;

  getNested = pathList: attrs:
    if pathList == []
    then attrs
    else if builtins.isAttrs attrs && builtins.hasAttr (builtins.head pathList) attrs
    then getNested (builtins.tail pathList) attrs.${builtins.head pathList}
    else null;

  mkOptionRecord = path: o: let
    typeAttr =
      if builtins.hasAttr "type" o
      then o.type
      else {};
    typeStr = getType typeAttr;
    defaultVal = tryOr null (
      if builtins.hasAttr "default" o
      then o.default
      else null
    );
    descVal = tryOr null (
      if builtins.hasAttr "description" o
      then o.description
      else null
    );
    pathList = if path == "" then [] else builtins.filter builtins.isString (builtins.split "\\." path);
    currentVal = tryOr null (
      if pathList == []
      then null
      else getNested pathList configRoot
    );
  in {
    name = path;
    type = typeStr;
    default = safeString defaultVal;
    description = safeString descVal;
    current = safeString currentVal;
  };

  walk = pathList: o: let
    path =
      if pathList == []
      then ""
      else builtins.concatStringsSep "." pathList;
  in
    if builtins.isAttrs o && builtins.hasAttr "_type" o && o._type == "option"
    then let
      record = mkOptionRecord path o;
      t = o.type or null;
      subOptionsSet = tryOr {} (
        if t != null && builtins.hasAttr "getSubOptions" t
        then t.getSubOptions pathList
        else {}
      );
      subRecords =
        if subOptionsSet == {}
        then []
        else let
          tname = tryOr "" (
            if t != null && builtins.hasAttr "name" t
            then t.name
            else ""
          );
          placeholder =
            if tname == "listOf"
            then ["*"]
            else if tname == "attrsOf" || tname == "lazyAttrsOf"
            then ["<name>"]
            else [];
          subPathList = pathList ++ placeholder;
        in
          walk subPathList subOptionsSet;
    in
      [record] ++ subRecords
    else if builtins.isAttrs o
    then let
      names = builtins.attrNames o;
      publicNames = builtins.filter (k: k != "_freeformOptions" && builtins.substring 0 1 k != "_") names;
    in
      builtins.concatLists (map (k: walk (pathList ++ [k]) o.${k}) publicNames)
    else [];
in
  walk [] root
