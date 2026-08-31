# Rathole certificateOnly: HTTP-only tunnel, gated on private hostname DNS.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.rathole-certificate-only = pkgs.runCommand "rathole-certificate-only" {} ''
      set -euo pipefail
      option=${./option.nix}
      impl=${./default.nix}

      if ! grep -q 'certificateOnly' "$option"; then
        echo "FAIL rathole option.nix must declare certificateOnly" >&2
        exit 1
      fi
      if ! grep -q 'privateHostnameDnsActive' "$impl"; then
        echo "FAIL rathole must gate certificateOnly with privateHostnameDnsActive" >&2
        exit 1
      fi
      if ! grep -qE 'optionalString[[:space:]]+\(!cfg\.certificateOnly\)' "$impl"; then
        echo "FAIL rathole client TOML must omit _https when certificateOnly is set" >&2
        exit 1
      fi
      if ! grep -q '_https' "$impl"; then
        echo "FAIL rathole must still define the _https client when certificateOnly is off" >&2
        exit 1
      fi
      if ! grep -q '_http' "$impl"; then
        echo "FAIL rathole must always define the _http client (Let's Encrypt)" >&2
        exit 1
      fi

      touch "$out"
    '';
  };
}
