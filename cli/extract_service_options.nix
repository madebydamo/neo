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

  safeString = x:
    if builtins.isString x
    then x
    else if x == null
    then "null"
    else builtins.toJSON x;

  getType = t:
    if builtins.hasAttr "name" t
    then t.name
    else if builtins.hasAttr "description" t
    then t.description
    else "unknown";

  tryOr = def: x: let
    res = builtins.tryEval x;
  in
    if res.success
    then res.value
    else def;

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
  in {
    name = path;
    type = typeStr;
    default = safeString defaultVal;
    description = safeString descVal;
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
