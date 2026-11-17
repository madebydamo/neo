# Secrets stay out of the Nix store: with every built-in service enabled and
# every secret setting (lib.neo.mkSecretOption, also inside submodules and one
# entry of each attrsOf) set to a value of its own (NEO-SENTINEL-<path>), no
# sentinel shows up in a container's or unit's environment, a unit's script
# or serviceConfig, an /etc text file or an activation script. Secrets go
# through neo.core.secrets.containerEnv / templates (AGENTS.md "Secrets").
# Encrypted configs catch placeholders the same way at build time
# (core/secrets.nix); this check covers plain configs and every service.
{
  config,
  inputs,
  lib,
  ...
}: let
  isOption = o: builtins.isAttrs o && (o._type or null) == "option";
  isAttrsOf = t: lib.elem (t.name or "") ["attrsOf" "lazyAttrsOf"];
  sentinel = path: "NEO-SENTINEL-${lib.concatStringsSep "." path}";
  set = lib.mkOverride 50;

  # Settings for every secret option below `opts`.
  secretValues = path: opts:
    lib.filterAttrs (_: v: v != {}) (lib.mapAttrs (k: o: let
      p = path ++ [k];
      t = o.type;
      elem = t.nestedTypes.elemType;
    in
      if isOption o
      then
        if (o.secret or false) && t.check (sentinel p)
        then set (sentinel p)
        # A secret table (core.git.tokens): one entry.
        else if (o.secret or false) && isAttrsOf t && elem.check "x"
        then set {"sentinel.example" = sentinel (p ++ ["*"]);}
        else if t.name == "submodule"
        then secretValues p (t.getSubOptions [])
        else if isAttrsOf t && elem.name == "submodule"
        then let
          inner = secretValues (p ++ ["*"]) (elem.getSubOptions []);
        in
          # One entry; its other required fields come from _all-enabled.nix.
          lib.optionalAttrs (inner != {}) {example = inner;}
        else {}
      else if builtins.isAttrs o && !(lib.hasPrefix "_" k)
      then secretValues p o
      else {})
    opts);

  all = import ./_all-enabled.nix {
    inherit inputs lib;
    neoFlake = config.flake;
    modules = [
      ({options, ...}: {neo = secretValues [] (removeAttrs options.neo ["_module"]);})
    ];
  };
  cfg = all.eval.config;
  secretPaths = lib.collect lib.isList (lib.mapAttrsRecursiveCond (a: !(a ? _type)) (p: _: p) (secretValues [] (removeAttrs all.eval.options.neo ["_module"])));

  text = v: builtins.unsafeDiscardStringContext (builtins.toJSON v);
  found = where: v: let
    m = builtins.match ".*(NEO-SENTINEL-[A-Za-z0-9.*_-]+).*" (text v);
  in
    lib.optional (m != null) "${builtins.head m} in ${where}";

  leaks =
    lib.concatLists (lib.mapAttrsToList (n: c: found "container ${n} environment" c.environment) cfg.virtualisation.oci-containers.containers)
    ++ lib.concatLists (lib.mapAttrsToList (n: u:
      found "unit ${n}" {
        inherit (u) environment script preStart postStart serviceConfig;
      })
    cfg.systemd.services)
    ++ lib.concatLists (lib.mapAttrsToList (n: e: found "/etc/${n}" (e.text or null)) cfg.environment.etc)
    ++ lib.concatLists (lib.mapAttrsToList (n: a: found "activation script ${n}" (a.text or a)) cfg.system.activationScripts);

  # The settings reached the services at all (templates hold them).
  used = lib.filter (t: lib.hasInfix "NEO-SENTINEL-" (text t.content or "")) (lib.attrValues cfg.neo.core.secrets.templates);
  problems =
    leaks
    ++ lib.optional (lib.length secretPaths < 40) "only ${toString (lib.length secretPaths)} secret settings found: did mkSecretOption change?"
    ++ lib.optional (used == []) "no secret template holds a sentinel: the settings did not reach the services";
in {
  perSystem = {pkgs, ...}: {
    checks.secret-leak = pkgs.runCommand "secret-leak" {} ''
      ${lib.concatMapStrings (p: "echo ${lib.escapeShellArg "FAIL ${p}: pass it through neo.core.secrets.containerEnv or neo.core.secrets.templates"} >&2\n") problems}
      ${lib.optionalString (problems != []) "exit 1"}
      echo "${toString (lib.length secretPaths)} secret settings, none in the store" > $out
    '';
  };
}
