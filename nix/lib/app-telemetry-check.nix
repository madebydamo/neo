# Unit checks for lib.neo.monitoring.appAlerts: the rules that a state with no
# series would otherwise break (NoData instead of Normal, silence that clears
# itself) are easy to undo by accident, so they are nailed down here.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    telemetry = (import ./app-telemetry.nix {inherit lib;}).libExtensions.appTelemetry.neo;
    alerts = telemetry.monitoring.appAlerts {
      service = "myapp";
      label = "MyApp";
      containers = ["myapp"];
      jobs.nightly = 26 * 3600;
      data.gps = 900;
    };
    has = id: s: lib.hasInfix s alerts.${id}.expr;
    cases = {
      # A count over logs returns no series while there is no error line:
      # without a fallback the rule flips between NoData and Normal.
      "errors-vector-0" = lib.hasSuffix "or on () vector(0)" alerts.errors.expr;
      "errors-loki" = alerts.errors.datasource == "loki";

      # Three rules per job: did not succeed, did not run, reports nothing.
      "job-rules" = lib.all (id: alerts ? ${id}) ["job-nightly" "job-nightly-not-run" "job-nightly-missing"];
      # "did not succeed" steps back when "did not run" has the case.
      "job-not-run-suppresses" = has "job-nightly" "unless on (host) (max by (host) (time() - neo_app_job_last_run_timestamp_seconds";
      "job-not-run-metric" = has "job-nightly-not-run" "neo_app_job_last_run_timestamp_seconds";
      "job-same-limit" = alerts.job-nightly.above == 93600 && alerts.job-nightly-not-run.above == 93600;

      # Silence: a short window with a long `for`, so one sample only resets
      # the clock instead of clearing the alert for the whole window.
      "job-missing-window" = has "job-nightly-missing" "absent_over_time(neo_app_job_last_success_timestamp_seconds{service=\"myapp\", job=\"nightly\"}[15m])";
      "job-missing-for" = alerts.job-nightly-missing.for == "26h";
      "data-missing-window" = has "data-gps-missing" "[15m])";
      # The limit is below six hours: a restart must not trip it.
      "data-missing-for" = alerts.data-gps-missing.for == "6h";
    };
    failed = lib.attrNames (lib.filterAttrs (_: ok: !ok) cases);
  in {
    checks.app-telemetry-alerts = pkgs.runCommand "app-telemetry-alerts" {} ''
      ${lib.optionalString (failed != []) ''
        echo "FAIL ${lib.concatStringsSep " " failed}" >&2
        exit 1
      ''}
      touch "$out"
    '';
  };
}
