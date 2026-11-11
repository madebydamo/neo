# Unit checks for lib.neo.cidr4.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    cidr = (import ./cidr.nix {inherit lib;}).libExtensions.cidr.neo.cidr4;
    cases = {
      "inside-cgnat" = cidr.overlaps "100.64.77.0/24" "100.64.0.0/10";
      "contains-cgnat" = cidr.overlaps "100.0.0.0/8" "100.64.0.0/10";
      "default-range-free" = !(cidr.overlaps "10.213.0.0/16" "100.64.0.0/10");
      "default-range-not-docker" = !(cidr.overlaps "10.213.0.0/16" "172.16.0.0/12");
      "docker-hit" = cidr.overlaps "172.20.0.0/16" "172.16.0.0/12";
      "edge-below" = !(cidr.overlaps "100.63.255.0/24" "100.64.0.0/10");
      "edge-above" = !(cidr.overlaps "100.128.0.0/16" "100.64.0.0/10");
      "invalid" = !(cidr.isValid "10.213.0/16") && !(cidr.isValid "10.213.0.0/33") && !(cidr.isValid "300.0.0.0/8");
      "valid" = cidr.isValid "10.213.0.0/16";
      "host-bits" = !(cidr.isNetwork "10.213.1.0/16") && cidr.isNetwork "10.213.0.0/16";
    };
    failed = lib.attrNames (lib.filterAttrs (_: ok: !ok) cases);
  in {
    checks.cidr4 = pkgs.runCommand "cidr4" {} ''
      ${lib.optionalString (failed != []) ''
        echo "FAIL ${lib.concatStringsSep " " failed}" >&2
        exit 1
      ''}
      touch "$out"
    '';
  };
}
