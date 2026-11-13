# Immich metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# Immich's Prometheus exporter is off by default: turned on (all metric groups)
# while a monitoring agent runs on this machine. immich-server serves the API
# process on :8081 and the background workers on :8082.
#
# Immich puts the job and queue into the metric name; the relabel rules turn
# them into labels at scrape time:
#   immich_jobs_<task>_<status>_total            → immich_jobs_total{task, status}
#   immich_queues_<queue>_<state>                → immich_queue_jobs{queue, state}
#   immich_<repo>_repository_<method>_duration_* → immich_repository_duration_*{repository, method}
{...}: {
  flake.modules.nixos.immich-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.immich;
    sel = lib.neo.grafana.appSelector "immich";

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

    relabel =
      nameToLabels "immich_jobs_(.+)_([a-z]+)_total" ["task" "status"] "immich_jobs_total"
      ++ nameToLabels "immich_queues_(.+)_(active|waiting|delayed|paused|failed|completed|prioritized)" ["queue" "state"] "immich_queue_jobs"
      ++ nameToLabels "immich_(.+)_repository_(.+)_duration_(bucket|count|sum)" ["repository" "method" "suffix"] "immich_repository_duration_$3";
  in {
    config = lib.mkIf cfg.enabled {
      virtualisation.oci-containers.containers.immich-server.environment = lib.mkIf config.neo.monitoring.collecting {
        IMMICH_TELEMETRY_INCLUDE = "all";
      };

      neo.monitoring.apps.immich = {
        metrics = {
          api = {
            container = "immich-server";
            port = 8081;
            relabel =
              relabel
              ++ [
                {
                  action = "labeldrop";
                  regex = "suffix|otel_scope_name";
                }
              ];
          };
          workers = {
            container = "immich-server";
            port = 8082;
            relabel =
              relabel
              ++ [
                {
                  action = "labeldrop";
                  regex = "suffix|otel_scope_name";
                }
              ];
          };
        };

        dashboards.overview = {
          title = "Immich";
          description = "Photo library, uploads, background jobs, machine learning and the server processes of Immich.";
          time = "now-7d";
          panels = with lib.neo.grafana; [
            (row "Library" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "Both Immich processes (API and background workers) answer.";
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
              title = "Users";
              w = 4;
              h = 4;
              targets = [(pq "max(immich_users_total{${sel}})" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Photos and videos added";
              w = 4;
              h = 4;
              description = "New assets in the selected time range.";
              targets = [(pq ''round(sum(increase(immich_repository_duration_count{${sel}, repository="asset", method="create"}[$__range])))'' "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Waiting jobs";
              w = 4;
              h = 4;
              description = "Background jobs (thumbnails, faces, search, …) not done yet.";
              targets = [(pq ''sum(immich_queue_jobs{${sel}, state=~"active|waiting|delayed"}) or vector(0)'' "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 1000;
                }
              ];
            })
            (stat {
              title = "Failed jobs";
              w = 4;
              h = 4;
              description = "Background jobs that failed in the selected time range.";
              targets = [(pq ''round(sum(increase(immich_jobs_total{${sel}, status="failed"}[$__range]))) or vector(0)'' "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Memory";
              w = 4;
              h = 4;
              description = "All Immich containers together.";
              targets = [(pq ''sum(container_memory_working_set_bytes{host=~"$host", name=~"immich-.*"})'' "")];
              unit = "bytes";
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (bars {
              title = "Photos and videos added";
              w = 24;
              h = 7;
              targets = [(pq ''sum(increase(immich_repository_duration_count{${sel}, repository="asset", method="create"}[$__interval]))'' "added")];
            })

            (row "Background jobs" null)
            (timeseries {
              title = "Queues";
              description = "Jobs per queue that are running or waiting.";
              targets = [(pq ''sum by (queue) (immich_queue_jobs{${sel}, state=~"active|waiting|delayed"})'' "{{queue}}")];
            })
            (bars {
              title = "Jobs done";
              targets = [(pq ''sum by (task) (increase(immich_jobs_total{${sel}, status="success"}[$__interval]))'' "{{task}}")];
            })
            (bars {
              title = "Jobs failed or skipped";
              targets = [(pq ''sum by (task, status) (increase(immich_jobs_total{${sel}, status!="success"}[$__interval]))'' "{{task}} {{status}}")];
            })
            (bargauge {
              title = "Jobs per task";
              description = "Successful jobs in the selected time range.";
              targets = [(pq ''sort_desc(sum by (task) (increase(immich_jobs_total{${sel}, status="success"}[$__range])))'' "{{task}}")];
              thresholds = thresholdsGreenRed [];
            })

            (row "Machine learning and database" null)
            (timeseries {
              title = "Machine learning calls";
              description = "Requests to the machine learning container (faces, smart search, text in photos).";
              targets = [(pq ''sum by (method) (rate(immich_repository_duration_count{${sel}, repository="machine_learning", method!~"tick|is_healthy|set_healthy|setup|teardown|check"}[$__rate_interval]))'' "{{method}}")];
              unit = "reqps";
            })
            (timeseries {
              title = "Machine learning time (95%)";
              targets = [(pq ''histogram_quantile(0.95, sum by (method, le) (rate(immich_repository_duration_bucket{${sel}, repository="machine_learning", method!~"tick|is_healthy|set_healthy|setup|teardown|check"}[$__rate_interval])))'' "{{method}}")];
              unit = "s";
            })
            (timeseries {
              title = "Slowest data access (95%)";
              description = "The ten repositories (database, storage, search) with the slowest 95th percentile.";
              targets = [(pq ''topk(10, histogram_quantile(0.95, sum by (repository, le) (rate(immich_repository_duration_bucket{${sel}, repository!="machine_learning"}[$__rate_interval]))))'' "{{repository}}")];
              unit = "s";
            })
            (timeseries {
              title = "Data access calls";
              targets = [(pq ''topk(10, sum by (repository) (rate(immich_repository_duration_count{${sel}, repository!="machine_learning"}[$__rate_interval])))'' "{{repository}}")];
              unit = "reqps";
            })

            (row "Processes and containers" null)
            (timeseries {
              title = "Processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name=~"immich-.*"}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name=~"immich-.*"})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-immich-.*"} |~ "(?i)(error|warn|exception|fail)"'')];
            })
          ];
        };
      };
    };
  };
}
