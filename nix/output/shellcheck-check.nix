# shellcheck over every shell file in nix/ and templates/. Most of them also
# build as writeShellApplication (shellcheck at build time), but only once a
# configuration uses them; scripts run with `bash file` or spliced into a
# unit's script are checked here alone.
{lib, ...}: let
  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions (map (lib.fileset.fileFilter (f: f.hasExt "sh")) [../. ../../templates]);
  };
in {
  perSystem = {pkgs, ...}: {
    checks.shellcheck =
      pkgs.runCommand "shellcheck" {
        nativeBuildInputs = [pkgs.shellcheck pkgs.findutils];
      } ''
        set -euo pipefail
        cd ${src}
        files=$(find . -name '*.sh' | sort)
        [ -n "$files" ]
        # Files without a shebang are bash (writeShellApplication, `bash file`).
        shellcheck --shell=bash --severity=style $files
        echo "$(wc -l <<<"$files") shell files ok"
        touch $out
      '';
  };
}
