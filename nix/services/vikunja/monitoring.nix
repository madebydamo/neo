# Vikunja metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# Vikunja (2.7.0, native NixOS service) serves Prometheus metrics at
# /api/v1/metrics once metrics.enabled is set; off by default, turned on while
# a monitoring agent runs on this machine. With metrics.username and
# metrics.password set it asks for basic auth (both empty = open). The
# credentials come in as VIKUNJA_METRICS_USERNAME / VIKUNJA_METRICS_PASSWORD
# (viper env, prefix vikunja, "." → "_") from a rendered env file, never the
# store. The agent runs on the host and scrapes 127.0.0.1:<port>.
#
# Exported (pkg/metrics, pkg/db, pkg/events):
#   vikunja_{user,project,task,team,files,attachments}_count   counts, cached 30s
#   vikunja_active_users, vikunja_active_link_shares            seen in the last 30s
#   go_sql_*{db_name}                                           connection pool
#   handler_execution_time_seconds{handler_name,success}        event handlers (watermill)
#   process_*, go_*                                             the Vikunja process
#
# The path is on the public port and under ^/api/ (tinyauth public), so SWAG
# hides it: hidePaths.
{...}: {
  flake.modules.nixos.vikunja-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vikunja;
    app = config.neo.monitoring.apps.vikunja;
    sel = lib.neo.grafana.appSelector "vikunja";
    metricsEnv = "vikunja-metrics.env";

    count = title: metric: description:
      lib.neo.grafana.stat {
        inherit title description;
        w = 4;
        h = 4;
        targets = [(lib.neo.grafana.pq "max(${metric}{${sel}})" "")];
        options.colorMode = "value";
        thresholds = lib.neo.grafana.thresholdsGreenRed [];
      };
  in {
    config = lib.mkIf cfg.enabled (lib.mkMerge [
      (lib.mkIf config.neo.monitoring.collecting {
        services.vikunja = {
          settings.metrics.enabled = true;
          environmentFiles = [config.neo.core.secrets.templates.${metricsEnv}.path];
        };

        neo.core.secrets.templates.${metricsEnv} = {
          env = {
            VIKUNJA_METRICS_USERNAME = "neo-metrics";
            VIKUNJA_METRICS_PASSWORD = app.scrapeToken;
          };
          restartUnits = ["vikunja.service"];
        };
      })
      {
        neo.monitoring.apps.vikunja = {
          hidePaths = ["/api/v1/metrics"];

          metrics.api = {
            address = "127.0.0.1:${toString cfg.port}";
            path = "/api/v1/metrics";
            auth = "basic";
          };

          dashboards.overview = {
            title = "Vikunja";
            description = "Tasks, projects, people using Vikunja, its database connections and the Vikunja process.";
            time = "now-7d";
            panels = with lib.neo.grafana; [
              (row "Overview" null)
              (stat {
                title = "Running";
                w = 4;
                h = 4;
                description = "Vikunja answers.";
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
              (count "Users" "vikunja_user_count" "Accounts on this Vikunja.")
              (count "Projects" "vikunja_project_count" "Projects of all users, archived ones included.")
              (count "Tasks" "vikunja_task_count" "Tasks of all users, done ones included.")
              (count "Teams" "vikunja_team_count" "Teams people share projects with.")
              (count "Files" "vikunja_files_count" "Stored files: task attachments, backgrounds and avatars.")

              (row "Activity" null)
              (timeseries {
                title = "People using Vikunja";
                description = "Signed-in users and visitors through a shared link, active in the last 30 seconds.";
                targets = [
                  (pq "max(vikunja_active_users{${sel}})" "Users")
                  (pq "max(vikunja_active_link_shares{${sel}})" "Shared links")
                ];
              })
              (timeseries {
                title = "Tasks and projects";
                description = "How the number of tasks and projects changes over time.";
                targets = [
                  (pq "max(vikunja_task_count{${sel}})" "Tasks")
                  (pq "max(vikunja_project_count{${sel}})" "Projects")
                ];
              })
              (timeseries {
                title = "Background events";
                description = "Work Vikunja does after a change (reminders, notifications, webhooks), per kind. Failed ones are marked.";
                targets = [(pq ''sum by (handler_name, success) (rate(handler_execution_time_seconds_count{${sel}}[$__rate_interval]))'' "{{handler_name}} (ok: {{success}})")];
                unit = "ops";
              })
              (timeseries {
                title = "Attachments";
                description = "Files attached to tasks.";
                targets = [(pq "max(vikunja_attachments_count{${sel}})" "Attachments")];
              })

              (row "Database" null)
              (timeseries {
                title = "Database connections";
                description = "Connections to the database that are open, busy and idle.";
                targets = [
                  (pq "sum(go_sql_open_connections{${sel}})" "Open")
                  (pq "sum(go_sql_in_use_connections{${sel}})" "Busy")
                  (pq "sum(go_sql_idle_connections{${sel}})" "Idle")
                ];
              })
              (timeseries {
                title = "Waiting for the database";
                description = "Time per second requests spent waiting for a free database connection. Above zero means the database is a bottleneck.";
                targets = [(pq ''sum(rate(go_sql_wait_duration_seconds_total{${sel}}[$__rate_interval]))'' "Waiting")];
                unit = "s";
              })

              (row "Process" null)
              (timeseries {
                title = "Processor";
                targets = [(pq ''sum(rate(process_cpu_seconds_total{${sel}}[$__rate_interval]))'' "Vikunja")];
                unit = "percentunit";
              })
              (timeseries {
                title = "Memory";
                targets = [
                  (pq "sum(process_resident_memory_bytes{${sel}})" "In use")
                  (pq "sum(go_memstats_heap_inuse_bytes{${sel}})" "Go heap")
                ];
                unit = "bytes";
              })
              (logs {
                title = "Errors and warnings";
                w = 24;
                h = 10;
                targets = [(lq ''{job="journal", host=~"$host", unit="vikunja.service"} |~ "(?i)(error|warn|critical|fail)"'')];
              })
            ];
          };
        };
      }
    ]);
  };
}
