# The web UI texts of built-in services follow AGENTS.md "Option texts" and
# "Service texts" where a machine can tell:
# - every service declares its upstream `license` (mkServiceMeta);
# - option descriptions (visible ones, nested ones too), taglines and service
#   descriptions name no environment variables (FOO_BAR, FOO=), no system paths
#   (/var/, /run/, /etc/) and no `services.x.y` option paths. Those belong in a
#   Nix comment; other settings are named by their label.
# Labels (`ui.label`) are not checked: most options without one have a name
# that reads fine humanized (password, port, users).
{
  config,
  inputs,
  lib,
  ...
}: let
  all = import ./_all-enabled.nix {
    inherit inputs lib;
    neoFlake = config.flake;
  };
  inherit (all.eval) options;
  cfg = all.eval.config;

  # Option path (services.<name>.…, `*` for entries) → why its text may break a rule.
  allowed = {
    # An example value for a folder field, not a path the service uses.
    "services.dufs.additionalMountPoints.*.localPath" = "example folder";
    "services.filebrowser.additionalMountPoints.*.localPath" = "example folder";
    "services.webtop.additionalMountPoints.*.localPath" = "example folder";
  };

  isOption = o: builtins.isAttrs o && (o._type or null) == "option";
  # Visible options below `o` as { path; text; }; into submodules, also behind
  # attrsOf / listOf / nullOr. Internal and invisible options are skipped whole.
  walk = path: o:
    if isOption o
    then
      lib.optionals (!(o.internal or false) && (o.visible or true) != false) (
        [
          {
            path = lib.concatStringsSep "." path;
            text = let
              d = o.description or "";
            in
              if builtins.isString d
              then d
              else d.text or "";
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

  rules = {
    "an environment variable" = "(.*[^A-Za-z0-9_])?([A-Z][A-Z0-9]*_[A-Z0-9_]+|[A-Z][A-Z0-9_]{2,}=)([^A-Za-z0-9_].*)?";
    "a system path" = ".*(/var/|/run/|/etc/).*";
    "an option path" = "(.*[^A-Za-z0-9_.])?(neo\\.)?services\\.[a-zA-Z0-9-]+\\.[a-zA-Z].*";
  };
  broken = where: text:
    lib.mapAttrsToList (what: _: "${where} names ${what}: ${text}")
    (lib.filterAttrs (_: re: builtins.match re (lib.replaceStrings ["\n"] [" "] text) != null) rules);

  optionProblems = lib.concatMap (n:
    lib.concatMap (o: lib.optionals (!(allowed ? ${o.path})) (broken o.path o.text))
    (walk ["services" n] (removeAttrs options.neo.services.${n} ["meta"])))
  all.names;

  metaProblems = lib.concatMap (n: let
    meta = cfg.neo.services.${n}.meta;
    license = meta.license or null;
  in
    lib.optional (license == null || (license.spdx or "") == "" || (license.url or "") == "")
    "services.${n}: mkServiceMeta has no license { spdx; url; }"
    ++ broken "services.${n} tagline" meta.tagline
    ++ broken "services.${n} description" meta.description)
  all.names;

  problems = metaProblems ++ optionProblems;
in {
  perSystem = {pkgs, ...}: {
    checks.service-texts = pkgs.runCommand "service-texts" {} ''
      ${lib.concatMapStrings (p: "echo ${lib.escapeShellArg "FAIL ${p}"} >&2\n") problems}
      ${lib.optionalString (problems != []) "exit 1"}
      echo "${toString (lib.length all.names)} services: texts ok" > $out
    '';
  };
}
