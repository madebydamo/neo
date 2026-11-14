# Relabel helpers for neo.monitoring.apps.<service>.metrics.<scrape>.relabel:
# lib.neo.monitoring.
#
#   relabel = lib.neo.monitoring.nameToLabels "immich_jobs_(.+)_([a-z]+)_total" ["task" "status"] "immich_jobs_total"
#     ++ lib.neo.monitoring.dropOtelScope;
{lib, ...}: {
  libExtensions.monitoring.neo.monitoring = {
    # Rules that move parts of a metric name into labels: the regex's groups
    # become `labels` (in order), the series is renamed to `name` (may use $1…).
    #   myapp_jobs_upload_failed_total → myapp_jobs_total{task="upload", status="failed"}
    nameToLabels = regex: labels: name:
      lib.imap1 (i: l: {
        inherit regex;
        targetLabel = l;
        replacement = "\$${toString i}";
      })
      labels
      ++ [
        {
          inherit regex;
          targetLabel = "__name__";
          replacement = name;
        }
      ];

    # OpenTelemetry exporters add otel_scope_* labels to every series and a
    # target_info series: noise for dashboards.
    dropOtelScope = [
      {
        action = "labeldrop";
        regex = "otel_scope_name|otel_scope_version|otel_scope_schema_url";
      }
      {
        action = "drop";
        regex = "target_info";
      }
    ];

    # nginx locations of neo.monitoring.apps.<service>.hidePaths: "/metrics"
    # (exact) or "^~ /prefix/" → 404, ahead of every other location.
    hidePathLocations = paths:
      lib.concatMapStrings (p: let
        match =
          if lib.hasPrefix "^~ " p
          then p
          else "= ${p}";
      in "  location ${match} { return 404; }\n")
      paths;
  };
}
