# flake-file dendritic/basic.nix mkDefaults flake-file.inputs.flake-file.url
# (github:vic/flake-file until eccac77, then github:denful/flake-file).
# Re-declaring that option at the same priority conflicts after flake update.
# Templates must leave the URL to dendritic so write-flake tracks upstream.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.template-flake-file-url = pkgs.runCommand "template-flake-file-url" {} ''
      set -euo pipefail
      homeserver=${../../templates/homeserver/modules/inputs.nix}
      plugin=${../../templates/plugin/modules/inputs.nix}
      homeserverFlake=${../../templates/homeserver/flake.nix}
      pluginFlake=${../../templates/plugin/flake.nix}

      fail=0
      for f in "$homeserver" "$plugin"; do
        if grep -qE 'flake-file\.url' "$f"; then
          echo "FAIL $f must not set flake-file.url (dendritic/basic.nix already mkDefaults it)" >&2
          fail=1
        fi
      done
      for f in "$homeserverFlake" "$pluginFlake"; do
        if ! grep -qE 'flake-file\.url = "github:[^"]+/flake-file"' "$f"; then
          echo "FAIL $f must keep a generated flake-file.url so flake init can load flake-file" >&2
          fail=1
        fi
      done
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi
      touch "$out"
    '';
  };
}
