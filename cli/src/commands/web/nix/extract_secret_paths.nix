# Settings paths of every lib.neo.mkSecretOption below neo (docs/design/secrets.md):
#   { paths = [ ["services" "rathole" "token"] ["services" "tinyauth" "oauth" "*" "clientSecret"] … ]; }
# "*" stands for any key of an attrsOf. A secret attrsOf option (core.git.tokens)
# is one path: its whole table is secret. Saves and `neo secrets split` move
# these values out of settings.toml into secrets.yaml.
{neoFlake}: let
  inherit (import ./extract_lib.nix {inherit neoFlake;}) f cfg tryOr;

  isOption = o: builtins.isAttrs o && (o._type or null) == "option";
  pubNames = o: builtins.filter (k: builtins.substring 0 1 k != "_") (builtins.attrNames o);

  walk = path: o:
    if isOption o
    then
      if tryOr false (o.secret or false)
      then [path]
      else sub path (tryOr null (o.type or null))
    else if builtins.isAttrs o
    then builtins.concatMap (k: walk (path ++ [k]) o.${k}) (pubNames o)
    else [];

  # Into submodules (also behind attrsOf / nullOr); lists stay opaque.
  sub = path: t: let
    name = tryOr "" (t.name or "");
    elem = tryOr null (t.nestedTypes.elemType or null);
  in
    if t == null
    then []
    else if name == "submodule"
    then walk path (tryOr {} (t.getSubOptions []))
    else if builtins.elem name ["attrsOf" "lazyAttrsOf"]
    then sub (path ++ ["*"]) elem
    else if name == "nullOr"
    then sub path elem
    else [];

  neoOptions = tryOr {} (f.nixosConfigurations.${cfg}.options.neo or {});
in {
  paths = tryOr [] (builtins.deepSeq (walk [] neoOptions) (walk [] neoOptions));
}
