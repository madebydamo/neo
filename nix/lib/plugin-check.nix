# The Nix plugin id rules against the vectors the Rust and widget tests run too.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    ids = import ../../templates/homeserver/modules/_plugin-ids.nix {inherit lib;};
    vectors = builtins.fromJSON (builtins.readFile ../../cli/src/utils/plugin_id_vectors.json);

    expectEq = name: actual: expected:
      lib.optionalString (actual != expected) ''
        echo ${lib.escapeShellArg "FAIL ${name}: got ${builtins.toJSON actual}, want ${builtins.toJSON expected}"} >&2
        fail=1
      '';
    pair = fn: what: v: expectEq "${what} of '${lib.elemAt v 0}'" (fn (lib.elemAt v 0)) (lib.elemAt v 1);
    checks =
      map (pair ids.label "label") vectors.labels
      ++ map (pair ids.sanitize "id") vectors.ids
      ++ map (v: expectEq "list ids" (lib.attrNames (ids.fromList v.urls)) (lib.sort (a: b: a < b) v.ids)) vectors.lists;
  in {
    checks.plugin-id-vectors = pkgs.runCommand "plugin-id-vectors" {} ''
      fail=0
      ${lib.concatStrings checks}
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi
      touch "$out"
    '';
  };
}
