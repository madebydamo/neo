# Unit checks for lib.neo.grafana: query letters past F (a panel with many targets),
# the recovery threshold of alert rules.
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
    rule = args:
      grafana.alertRule ({
          uid = "t";
          title = "t";
          expr = "up";
          summary = "s";
        }
        // args);
    cond = r: lib.head (lib.last r.data).model.conditions;
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
      "no-recover" = !(cond (rule {above = 10;})) ? unloadEvaluator;
      "recover-above" =
        cond (rule {
          above = 10;
          recover = 5;
        })
        == {
          evaluator = {
            type = "gt";
            params = [10];
          };
          unloadEvaluator = {
            type = "lt";
            params = [5];
          };
        };
      "recover-below" =
        (cond (rule {
          below = 1;
          recover = 2;
        })).unloadEvaluator
        == {
          type = "gt";
          params = [2];
        };
      "recover-wrong-side" =
        !(builtins.tryEval (builtins.deepSeq (rule {
            above = 10;
            recover = 20;
          })
          true)).success;
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
