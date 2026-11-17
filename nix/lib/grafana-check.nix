# Unit checks for lib.neo.grafana: query letters past F (a panel with many targets).
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    grafana = (import ./grafana.nix {inherit lib;}).libExtensions.grafana.neo.grafana;
    panel = grafana.timeseries {
      title = "many";
      targets = lib.genList (i: grafana.pq "up" "q${toString i}") 30;
    };
    cases = {
      "first" = grafana.refId 0 == "A";
      "z" = grafana.refId 25 == "Z";
      "aa" = grafana.refId 26 == "AA";
      "az" = grafana.refId 51 == "AZ";
      "ba" = grafana.refId 52 == "BA";
      "zz" = grafana.refId 701 == "ZZ";
      "aaa" = grafana.refId 702 == "AAA";
      "panel" = map (t: t.refId) panel.targets == lib.genList grafana.refId 30;
      "unique" = lib.unique (lib.genList grafana.refId 800) == lib.genList grafana.refId 800;
    };
    failed = lib.attrNames (lib.filterAttrs (_: ok: !ok) cases);
  in {
    checks.grafana-lib = pkgs.runCommand "grafana-lib" {} ''
      ${lib.optionalString (failed != []) ''
        echo "FAIL ${lib.concatStringsSep " " failed}" >&2
        exit 1
      ''}
      touch "$out"
    '';
  };
}
