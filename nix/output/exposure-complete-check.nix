# Every port open to the outside (NixOS firewall, per-interface rules, Docker
# ports not bound to loopback) is described by the module that opens it in
# neo.exposure.ports, so the Access page lists all of them with what protects
# them (nix/modules/core/exposure.nix). Checked with every service enabled.
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
  report = all.eval.config.neo.exposure.report;
  problems = map (p: "${p} is open but no module describes it in neo.exposure.ports") report.undescribed;
in {
  perSystem = {pkgs, ...}: {
    checks.exposure-complete = pkgs.runCommand "exposure-complete" {} ''
      ${lib.concatMapStrings (p: "echo ${lib.escapeShellArg "FAIL ${p}"} >&2\n") problems}
      ${lib.optionalString (problems != []) "exit 1"}
      echo "${toString (lib.length report.ports)} open ports, all described" > $out
    '';
  };
}
