# Dawarich metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# Dawarich's exporter (Yabeda) is off by default: turned on while a monitoring
# agent runs on this machine. With PROMETHEUS_EXPORTER_ENABLED=true:
#   - dawarich-sidekiq serves yabeda-sidekiq (sidekiq_*) and the raw data
#     archive metrics (dawarich_archive_*) on 0.0.0.0:9394 (WEBrick,
#     config/initializers/sidekiq.rb; PROMETHEUS_EXPORTER_PORT, default 9394).
#   - dawarich serves /metrics on :3000 (Rails, Puma, plus the Sidekiq numbers
#     fetched from SIDEKIQ_METRICS_URL). Not scraped: the Rails host check and
#     force_ssl stand in the way, and Sidekiq has all the job metrics. Behind
#     tinyauth on the public site, and basic auth as well.
# Both check basic auth against METRICS_USERNAME / METRICS_PASSWORD (Dawarich's
# MetricsBasicAuth); the web process also uses them to fetch from Sidekiq.
#
# Jobs queued from the web process are counted there, so
# sidekiq_jobs_enqueued_total here only has jobs queued by other jobs.
{...}: {
  flake.modules.nixos.dawarich-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.dawarich;
    app = config.neo.monitoring.apps.dawarich;
    collecting = config.neo.monitoring.collecting;
    sel = lib.neo.grafana.appSelector "dawarich";
    username = "neo-metrics";

    metricsEnv = {
      PROMETHEUS_EXPORTER_ENABLED = "true";
      METRICS_USERNAME = username;
      SIDEKIQ_METRICS_URL = "http://dawarich-sidekiq:9394/metrics";
    };
  in {
    config = lib.mkIf cfg.enabled {
      virtualisation.oci-containers.containers = {
        dawarich.environment = lib.mkIf collecting metricsEnv;
        dawarich-sidekiq.environment = lib.mkIf collecting metricsEnv;
      };

      neo.core.secrets.containerEnv = lib.mkIf collecting {
        dawarich.METRICS_PASSWORD = app.scrapeToken;
        dawarich-sidekiq.METRICS_PASSWORD = app.scrapeToken;
      };

      neo.monitoring.apps.dawarich = {
        # The web app serves /metrics on its public site while the exporter is on.
        hidePaths = ["/metrics"];
        metrics.sidekiq = {
          container = "dawarich-sidekiq";
          port = 9394;
          auth = "basic";
          inherit username;
        };

        dashboards.overview = {
          title = "Dawarich";
          description = "Background work of Dawarich: imports, place names, statistics and the location history archive.";
          time = "now-7d";
          panels = with lib.neo.grafana; [
            (row "Background jobs" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "The Dawarich background worker answers.";
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
              title = "Jobs done";
              w = 4;
              h = 4;
              description = "Background jobs finished without error in the selected time range.";
              targets = [(pq "round(sum(increase(sidekiq_jobs_success_total{${sel}}[$__range]))) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Failed jobs";
              w = 4;
              h = 4;
              description = "Background jobs that ended with an error in the selected time range. Most are tried again later.";
              targets = [(pq "round(sum(increase(sidekiq_jobs_failed_total{${sel}}[$__range]))) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Waiting jobs";
              w = 4;
              h = 4;
              description = "Jobs in all queues that have not started yet.";
              targets = [(pq "sum(sidekiq_jobs_waiting_count{${sel}}) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 1000;
                }
              ];
            })
            (stat {
              title = "Waiting for retry";
              w = 4;
              h = 4;
              description = "Failed jobs that will be tried again later.";
              targets = [(pq "sum(sidekiq_jobs_retry_count{${sel}}) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Given up";
              w = 4;
              h = 4;
              description = "Jobs that failed too often and will not be tried again (Sidekiq's dead jobs).";
              targets = [(pq "sum(sidekiq_jobs_dead_count{${sel}}) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (bars {
              title = "Jobs run";
              description = "Background jobs started, by kind of job.";
              targets = [(pq "sum by (worker) (increase(sidekiq_jobs_executed_total{${sel}}[$__interval]))" "{{worker}}")];
            })
            (bars {
              title = "Jobs failed";
              description = "Background jobs that ended with an error, by kind of job.";
              targets = [(pq "sum by (worker) (increase(sidekiq_jobs_failed_total{${sel}}[$__interval]))" "{{worker}}")];
            })
            (bars {
              title = "Jobs queued by other jobs";
              description = "New jobs that background jobs added, by kind of job. Jobs you start in the web app are not counted here.";
              targets = [(pq "sum by (worker) (increase(sidekiq_jobs_enqueued_total{${sel}}[$__interval]))" "{{worker}}")];
            })
            (timeseries {
              title = "Busy workers";
              description = "Jobs running at the same moment, and worker processes that are up.";
              targets = [
                (pq "sum(sidekiq_active_workers_count{${sel}})" "jobs running")
                (pq "sum(sidekiq_active_processes{${sel}})" "worker processes")
              ];
              decimals = 0;
            })

            (row "Queues" null)
            (timeseries {
              title = "Waiting jobs per queue";
              targets = [(pq "sum by (queue) (sidekiq_jobs_waiting_count{${sel}})" "{{queue}}")];
              decimals = 0;
            })
            (timeseries {
              title = "Queue delay";
              description = "How long the oldest waiting job in each queue has been waiting.";
              targets = [(pq "max by (queue) (sidekiq_queue_latency{${sel}})" "{{queue}}")];
              unit = "s";
            })
            (timeseries {
              title = "Wait before start (95%)";
              description = "Time from queuing a job to its start, by kind of job.";
              targets = [(pq "histogram_quantile(0.95, sum by (worker, le) (rate(sidekiq_job_latency_seconds_bucket{${sel}}[$__rate_interval])))" "{{worker}}")];
              unit = "s";
            })
            (timeseries {
              title = "Job duration (95%)";
              description = "How long jobs take to run, by kind of job.";
              targets = [(pq "histogram_quantile(0.95, sum by (worker, le) (rate(sidekiq_job_runtime_seconds_bucket{${sel}}[$__rate_interval])))" "{{worker}}")];
              unit = "s";
            })
            (timeseries {
              title = "Longest running job";
              description = "How long the oldest job still running has been at it. A line that keeps climbing points to a stuck job.";
              targets = [(pq "max by (worker) (sidekiq_running_job_runtime_seconds{${sel}}) > 0" "{{worker}}")];
              unit = "s";
            })
            (timeseries {
              title = "Retries and given-up jobs";
              description = "Jobs waiting to be tried again, jobs given up on, and jobs planned for later.";
              targets = [
                (pq "sum(sidekiq_jobs_retry_count{${sel}})" "waiting for retry")
                (pq "sum(sidekiq_jobs_dead_count{${sel}})" "given up")
                (pq "sum(sidekiq_jobs_scheduled_count{${sel}})" "planned for later")
              ];
              decimals = 0;
            })

            (row "Location history archive" null)
            (text {
              content = "Once a month Dawarich packs old raw location data into compressed archive files, checks them every day and removes the packed data from the database a week later. Empty panels mean nothing has been archived yet.";
            })
            (stat {
              title = "Archive runs failed";
              w = 6;
              h = 4;
              description = "Archive, check, clear or restore runs that failed in the selected time range.";
              targets = [(pq ''round(sum(increase(dawarich_archive_operations_total{${sel}, status="failure"}[$__range]))) or vector(0)'' "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Failed archive checks";
              w = 6;
              h = 4;
              description = "Checks of an archive file that found a problem in the selected time range.";
              targets = [(pq "round(sum(increase(dawarich_archive_verification_failures_total{${sel}}[$__range]))) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Count mismatches";
              w = 6;
              h = 4;
              description = "Times the number of points in an archive did not match the database, in the selected time range.";
              targets = [(pq "round(sum(increase(dawarich_archive_count_mismatches_total{${sel}}[$__range]))) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Points archived";
              w = 6;
              h = 4;
              description = "Location points packed into archive files in the selected time range.";
              targets = [(pq ''round(sum(increase(dawarich_archive_points_total{${sel}, operation="added"}[$__range]))) or vector(0)'' "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (bars {
              title = "Archive runs";
              description = "Archive, check, clear and restore runs, by outcome.";
              targets = [(pq "sum by (operation, status) (increase(dawarich_archive_operations_total{${sel}}[$__interval]))" "{{operation}} {{status}}")];
            })
            (bars {
              title = "Failed archive checks by check";
              targets = [(pq "sum by (check) (increase(dawarich_archive_verification_failures_total{${sel}}[$__interval]))" "{{check}}")];
            })

            (row "Containers" null)
            (timeseries {
              title = "Processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name=~"dawarich.*"}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name=~"dawarich.*"})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-dawarich.*"} |~ "(?i)(error|warn|exception|fail)"'')];
            })
          ];
        };
      };
    };
  };
}
