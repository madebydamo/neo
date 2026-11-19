# Defaults that come from another setting are declared the same way
# everywhere (docs/design/access-and-defaults.md), so the web UI can show where
# a value comes from:
# - a visible option whose default contains the server name or the domain
#   (evaluated with sentinel values) is a lib.neo.mkInheritedOption;
# - a description that says what an empty value falls back to ("Empty uses …",
#   "leave empty to use …") belongs to an option with ui.emptyMeans or one
#   that inherits.
{
  config,
  inputs,
  lib,
  ...
}: let
  host = "inheritsentinelhost";
  domain = "inheritsentineldomain.test";
  all = import ./_all-enabled.nix {
    inherit inputs lib;
    neoFlake = config.flake;
    modules = [
      {
        neo.core.hostname = lib.mkForce host;
        neo.services.swag.domain = lib.mkForce domain;
      }
    ];
  };
  inherit (all.eval) options;

  # Option path → why its default may carry a sentinel without inheriting.
  allowed = {
  };

  isOption = o: builtins.isAttrs o && (o._type or null) == "option";
  walk = path: o:
    if isOption o
    then
      lib.optionals (!(o.internal or false) && (o.visible or true) != false && !(o.readOnly or false)) (
        [
          {
            path = lib.concatStringsSep "." path;
            inherit o;
          }
        ]
        ++ sub path (o.type or null)
      )
    else if builtins.isAttrs o
    then lib.concatMap (k: walk (path ++ [k]) o.${k}) (lib.filter (k: !(lib.hasPrefix "_" k)) (lib.attrNames o))
    else [];
  sub = path: t:
    if t == null
    then []
    else if t.name == "submodule"
    then walk path (t.getSubOptions [])
    else if t.name == "nullOr"
    then sub path t.nestedTypes.elemType
    else if lib.elem t.name ["attrsOf" "lazyAttrsOf" "listOf"]
    then sub (path ++ ["*"]) t.nestedTypes.elemType
    else [];

  # Strings in a default (scalars, lists, attrs), [] when it does not evaluate.
  strings = v: let
    r = builtins.tryEval v;
  in
    if !r.success
    then []
    else if builtins.isString r.value
    then let
      s = builtins.tryEval (builtins.deepSeq r.value r.value);
    in
      lib.optional s.success s.value
    else if builtins.isList r.value
    then lib.concatMap strings r.value
    else if builtins.isAttrs r.value && !(lib.isDerivation r.value)
    then lib.concatMap (k: strings r.value.${k}) (lib.attrNames r.value)
    else [];

  description = o: let
    d = o.description or "";
  in
    if builtins.isString d
    then d
    else d.text or "";
  fallbackText = "(.*[^A-Za-z])?([Ee]mpty uses|[Ll]eave (it )?empty to use|[Ee]mpty: use|[Ee]mpty = use).*";

  problems =
    lib.concatMap ({
      path,
      o,
    }: let
      inherits = o ? inherits;
      emptyMeans = (o.ui or {}) ? emptyMeans;
      derived = lib.any (s: lib.hasInfix host s || lib.hasInfix domain s) (strings (o.default or null));
      text = lib.replaceStrings ["\n"] [" "] (description o);
    in
      lib.optional (derived && !inherits && !(allowed ? ${path}))
      "${path}: the default comes from the server name or domain; declare it with lib.neo.mkInheritedOption"
      ++ lib.optional (builtins.match fallbackText text != null && !inherits && !emptyMeans)
      "${path}: the description names what empty falls back to; use lib.neo.mkInheritedOption or ui.emptyMeans")
    (walk ["services"] options.neo.services
      ++ walk ["core"] options.neo.core
      ++ walk ["access"] (options.neo.access or {}));
in {
  perSystem = {pkgs, ...}: {
    checks.inherited-defaults = pkgs.runCommand "inherited-defaults" {} ''
      ${lib.concatMapStrings (p: "echo ${lib.escapeShellArg "FAIL ${p}"} >&2\n") problems}
      ${lib.optionalString (problems != []) "exit 1"}
      echo "inherited defaults ok" > $out
    '';
  };
}
