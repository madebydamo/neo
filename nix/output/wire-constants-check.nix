# Names written in one language and read in another, which no import can share:
# on-disk and wire formats between Nix, shell and Rust. Each must stay in every file.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    constants = {
      # ZFS snapshot prefix: taken by backup.sh, allowlisted by the store gate, read by restore.
      "neo-backup-" = [
        ../services/backup/backup.sh
        ../../cli/src/bin/neo-store-gate.rs
        ../../cli/src/commands/restore/backup.rs
      ];
      # Encrypted backup's data key: written by backup.sh, read by restore.
      "neo-backup.age" = [
        ../services/backup/backup.sh
        ../../cli/src/commands/restore/backup.rs
      ];
      # Placeholder of an encrypted secret: from Nix, masked and resolved in Rust.
      "@NEO_SECRET:" = [
        ../lib/secrets.nix
        ../../cli/src/utils/secrets.rs
      ];
      # Marks setup units; the web UI's extract cannot reach lib.neo.
      "X-Neo-Setup" = [
        ../lib/setup-service.nix
        ../../cli/src/commands/web/nix/extract_service_options.nix
      ];
    };
    checkOne = constant: file: ''
      if ! grep -qF -- ${lib.escapeShellArg constant} ${file}; then
        echo ${lib.escapeShellArg "FAIL ${constant} is missing from ${baseNameOf (toString file)}"} >&2
        fail=1
      fi
    '';
  in {
    checks.wire-constants = pkgs.runCommand "wire-constants" {} ''
      fail=0
      ${lib.concatStrings (lib.concatLists (lib.mapAttrsToList (c: map (checkOne c)) constants))}
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi
      touch "$out"
    '';
  };
}
