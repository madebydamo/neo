# Collabora metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# coolwsd serves Prometheus text on /cool/getMetrics (wsd/Admin.cpp,
# wsd/AdminModel.cpp getMetrics). It normally wants the admin console login;
# while a monitoring agent runs on this machine it is opened without one
# (security.enable_metrics_unauthenticated) and stops logging every fetch.
# No admin password on purpose: it would turn the admin console into a live
# login on the internet. SWAG hides the endpoint and the admin console
# (hidePaths), so only the agent on the Docker network reaches it.
#
# The image is collabora/code:latest, so names may drift; every query falls
# back to nothing or 0. Names as of co-25.04, renamed collabora_<name> here:
#   kit_assigned_count                             documents open (one kit each)
#   document_active_views_{all,active}_count_total people in open documents (all / active)
#   document_{active,all}_view_load_duration_{average,max}_milliseconds
#   document_all_wopi_{download,upload}_duration_average_milliseconds
#   document_all_{sent_to,received_from}_clients_total_bytes   grow until restart
#   kit_{segfault,killed_oom,lost_terminated,killed}_count     grow until restart
#   error_<kind>                                   → collabora_errors{kind}
#   global_memory_{used,available}_bytes, coolwsd/forkit_memory_used_bytes, kit_memory_used_total_bytes
# doc_* (one series per open document) carry file names and are dropped.
{...}: {
  flake.modules.nixos.collabora-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.collabora;
    sel = lib.neo.grafana.appSelector "collabora";
    m = name: "collabora_${name}{${sel}}";
  in {
    config = lib.mkIf cfg.enabled {
      # CODE 26.04+ ignores extra_params; options go on cmd (see default.nix).
      virtualisation.oci-containers.containers.collabora.cmd = lib.mkIf config.neo.monitoring.collecting [
        "--o:security.enable_metrics_unauthenticated=true"
        "--o:admin_console.logging.metrics_fetch=false"
      ];

      neo.monitoring.apps.collabora = {
        hidePaths = ["/cool/getMetrics" "/cool/adminws" "^~ /browser/dist/admin/"];

        metrics.main = {
          container = "collabora";
          port = 9980;
          path = "/cool/getMetrics";
          network = lib.neo.docker.serviceNetwork "collabora";
          relabel =
            [
              {
                action = "drop";
                regex = "doc_.*";
              }
              # Only Collabora's own names: up and scrape_* pass this stage too.
              {
                regex = "((?:global|coolwsd|forkit|kit|document|error)_.+)";
                targetLabel = "__name__";
                replacement = "collabora_$1";
              }
            ]
            ++ lib.neo.monitoring.nameToLabels "collabora_error_(.+)" ["kind"] "collabora_errors";
        };

        dashboards.overview = {
          title = "Collabora";
          description = "Office documents open in Collabora, the people editing them, how fast they load and save, and crashes.";
          time = "now-7d";
          panels = with lib.neo.grafana; [
            (row "Documents" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "Collabora answers.";
              targets = [(pq "min(up{${sel}})" "")];
              mappings = upDown;
              thresholds = {
                mode = "absolute";
                steps = [
                  {
                    color = "red";
                    value = null;
                  }
                  {
                    color = "green";
                    value = 1;
                  }
                ];
              };
            })
            (stat {
              title = "Documents open";
              w = 4;
              h = 4;
              targets = [(pq "sum(${m "kit_assigned_count"}) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "People in documents";
              w = 4;
              h = 4;
              description = "Everyone who has a document open, also in another tab.";
              targets = [(pq "sum(${m "document_active_views_all_count_total"}) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Crashes";
              w = 4;
              h = 4;
              description = "Document processes that crashed or ran out of memory in the selected time range.";
              targets = [(pq "round((sum(increase(${m "kit_segfault_count"}[$__range])) or vector(0)) + (sum(increase(${m "kit_killed_oom_count"}[$__range])) or vector(0)))" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Load time";
              w = 4;
              h = 4;
              description = "How long opening a document took on average since Collabora started.";
              targets = [(pq "max(${m "document_all_view_load_duration_average_milliseconds"}) / 1000" "")];
              unit = "s";
              decimals = 1;
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 5;
                }
              ];
            })
            (stat {
              title = "Memory";
              w = 4;
              h = 4;
              description = "Memory Collabora uses for all open documents together.";
              targets = [(pq "max(${m "global_memory_used_bytes"})" "")];
              unit = "bytes";
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (timeseries {
              title = "Documents and people";
              description = "Documents open, everyone in them, and those who are active right now (not idle).";
              targets = [
                (pq "sum(${m "kit_assigned_count"})" "documents")
                (pq "sum(${m "document_active_views_all_count_total"})" "people")
                (pq "sum(${m "document_active_views_active_count_total"})" "active people")
              ];
              decimals = 0;
            })
            (timeseries {
              title = "Data to and from browsers";
              description = "What Collabora sends to the people editing and receives from them.";
              targets = [
                (pq "sum(rate(${m "document_all_sent_to_clients_total_bytes"}[$__rate_interval]))" "sent")
                (pq "sum(rate(${m "document_all_received_from_clients_total_bytes"}[$__rate_interval]))" "received")
              ];
              unit = "Bps";
            })

            (row "Speed" null)
            (timeseries {
              title = "Load time";
              description = "How long opening a document took, for the documents open now (average and slowest) and on average since Collabora started.";
              targets = [
                (pq "max(${m "document_active_view_load_duration_average_milliseconds"}) / 1000" "open now, average")
                (pq "max(${m "document_active_view_load_duration_max_milliseconds"}) / 1000" "open now, slowest")
                (pq "max(${m "document_all_view_load_duration_average_milliseconds"}) / 1000" "since start, average")
              ];
              unit = "s";
            })
            (timeseries {
              title = "Fetching and saving to Nextcloud";
              description = "How long Collabora takes on average to fetch a document from Nextcloud and to save it back, since Collabora started.";
              targets = [
                (pq "max(${m "document_all_wopi_download_duration_average_milliseconds"}) / 1000" "fetch")
                (pq "max(${m "document_all_wopi_upload_duration_average_milliseconds"}) / 1000" "save")
              ];
              unit = "s";
            })

            (row "Problems" null)
            (bars {
              title = "Crashes and stopped documents";
              description = "Document processes that crashed, ran out of memory, were stopped for using too much, or went missing.";
              targets = [
                (pq "sum(increase(${m "kit_segfault_count"}[$__interval]))" "crashed")
                (pq "sum(increase(${m "kit_killed_oom_count"}[$__interval]))" "out of memory")
                (pq "sum(increase(${m "kit_killed_count"}[$__interval]))" "stopped")
                (pq "sum(increase(${m "kit_lost_terminated_count"}[$__interval]))" "lost")
              ];
            })
            (bars {
              title = "Errors";
              description = "Failed requests by reason, e.g. Nextcloud not reachable (storage connection) or a link that is no longer valid (unauthorized request).";
              targets = [(pq "sum by (kind) (increase(${m "errors"}[$__interval])) > 0" "{{kind}}")];
            })

            (row "Processes and containers" null)
            (timeseries {
              title = "Collabora memory";
              w = 24;
              description = "Memory per part of Collabora, and how much it may use before it refuses new documents.";
              targets = [
                (pq "max(${m "coolwsd_memory_used_bytes"})" "server")
                (pq "max(${m "forkit_memory_used_bytes"})" "document starter")
                (pq "max(${m "kit_memory_used_total_bytes"})" "documents")
                (pq "max(${m "global_memory_available_bytes"})" "limit")
              ];
              unit = "bytes";
            })
            (timeseries {
              title = "Processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name="collabora"}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Container memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name="collabora"})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-collabora.service|collabora-setup.service"} |~ "(?i)(error|warn|exception|fail)"'')];
            })
          ];
        };
      };
    };
  };
}
