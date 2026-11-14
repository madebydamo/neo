# Monitoring's own metrics and the "Monitoring health" dashboard
# (neo.monitoring.apps, nix/modules/core/monitoring.nix): does the logging
# itself work (ISO 27001 A.8.15 / A.8.16)?
#
# All three serve /metrics without auth and without switches:
#   prometheus  127.0.0.1:<prometheusPort>/metrics (published on loopback, default.nix)
#   loki        127.0.0.1:<lokiPort>/metrics       (same)
#   grafana     grafana:3000/metrics on the monitoring network
# Grafana's /metrics would also answer on its public site: hidePaths 404s it.
#
# The agents' own delivery counters (loki_write_*, prometheus_remote_storage_*)
# come from every machine under job="alloy" (monitoring-agent/_alloy.nix), so
# those panels are not filtered by server.
#
# Metric names checked against prom/prometheus:v3 (3.16), grafana/loki:3 (3.7)
# and grafana/grafana:latest (13.3). Most counters with labels only appear after
# the first event, hence the `or vector(0)`.
{...}: {
  flake.modules.nixos.monitoring-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.monitoring;
    sel = lib.neo.grafana.appSelector "monitoring";
    # The agent's self-scrape honours Alloy's own labels: job="integrations/self".
    agent = ''job=~"alloy|integrations/self"'';
    containers = ''host=~"$host", name=~"grafana|prometheus|loki"'';
  in {
    config = lib.mkIf cfg.enabled {
      neo.monitoring.apps.monitoring = {
        metrics = {
          prometheus.address = "127.0.0.1:${toString cfg.prometheusPort}";
          loki.address = "127.0.0.1:${toString cfg.lokiPort}";
          grafana = {
            container = "grafana";
            port = 3000;
            network = lib.neo.docker.serviceNetwork "monitoring";
          };
        };

        hidePaths = ["/metrics"];

        dashboards.health = {
          title = "Monitoring health";
          description = "Whether logs and numbers from every server arrive and are kept, and whether alerts reach you.";
          time = "now-7d";
          panels = with lib.neo.grafana; [
            (row "At a glance" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "Grafana, Prometheus and Loki all answer.";
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
              title = "Log lines received";
              w = 4;
              h = 4;
              description = "Log lines Loki took in during the selected time range.";
              targets = [(pq "round(sum(increase(loki_distributor_lines_received_total{${sel}}[$__range]))) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Log lines refused";
              w = 4;
              h = 4;
              description = "Log lines Loki threw away (too old, too many, too long). These are lost.";
              targets = [(pq "round(sum(increase(loki_discarded_samples_total{${sel}}[$__range]))) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Deliveries refused";
              w = 4;
              h = 4;
              description = "Batches of numbers from the agents that Prometheus did not accept.";
              targets = [(pq ''round(sum(increase(prometheus_http_requests_total{${sel}, handler="/api/v1/write", code!~"2.."}[$__range]))) or vector(0)'' "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Lost on the way";
              w = 4;
              h = 4;
              description = "Log lines and numbers the agents on all servers gave up sending.";
              targets = [
                (pq "round((sum(increase(loki_write_dropped_entries_total{${agent}}[$__range])) or vector(0)) + (sum(increase(prometheus_remote_storage_samples_failed_total{${agent}}[$__range])) or vector(0)))" "")
              ];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Alert messages failed";
              w = 4;
              h = 4;
              description = "Alert messages Grafana could not deliver (to the assistant or another contact point).";
              targets = [(pq "round(sum(increase(grafana_alerting_notifications_failed_total{${sel}}[$__range]))) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })

            (row "What arrives" null)
            (timeseries {
              title = "Log lines sent per server";
              description = "What the agent on each server delivers to Loki.";
              targets = [(pq "sum by (host) (rate(loki_write_sent_entries_total{${agent}}[$__rate_interval]))" "{{host}}")];
              unit = "short";
            })
            (timeseries {
              title = "Numbers sent per server";
              description = "Measurements the agent on each server delivers to Prometheus, per second.";
              targets = [(pq "sum by (host) (rate(prometheus_remote_storage_samples_total{${agent}}[$__rate_interval]))" "{{host}}")];
              unit = "short";
            })
            (bars {
              title = "Lost by the agents";
              description = "Log lines and numbers an agent dropped after retrying, per server. Should stay empty.";
              targets = [
                (pq "sum by (host) (increase(loki_write_dropped_entries_total{${agent}}[$__interval])) > 0" "{{host}} log lines")
                (pq "sum by (host) (increase(prometheus_remote_storage_samples_failed_total{${agent}}[$__interval])) > 0" "{{host}} numbers")
              ];
            })
            (timeseries {
              title = "Stored";
              description = "Log lines Loki and measurements Prometheus store, per second.";
              targets = [
                (pq "sum(rate(loki_distributor_lines_received_total{${sel}}[$__rate_interval]))" "log lines")
                (pq "sum(rate(prometheus_tsdb_head_samples_appended_total{${sel}}[$__rate_interval]))" "numbers")
              ];
              unit = "short";
            })

            (row "What was refused" null)
            (bars {
              title = "Log lines refused, by reason";
              description = "Why Loki threw lines away, e.g. too old (greater_than_max_sample_age) or over the rate limit (rate_limited).";
              targets = [(pq "sum by (reason) (increase(loki_discarded_samples_total{${sel}}[$__interval])) > 0" "{{reason}}")];
            })
            (bars {
              title = "Numbers refused";
              description = "Deliveries Prometheus answered with an error, and measurements it dropped because their time did not fit.";
              targets = [
                (pq ''sum by (code) (increase(prometheus_http_requests_total{${sel}, handler="/api/v1/write", code!~"2.."}[$__interval])) > 0'' "delivery error {{code}}")
                (pq "sum(increase(prometheus_tsdb_out_of_order_samples_total{${sel}}[$__interval])) > 0" "out of order")
                (pq "sum(increase(prometheus_tsdb_out_of_bound_samples_total{${sel}}[$__interval])) > 0" "out of range")
                (pq "sum(increase(prometheus_tsdb_too_old_samples_total{${sel}}[$__interval])) > 0" "too old")
              ];
            })

            (row "Storage" null)
            (timeseries {
              title = "Series in memory";
              description = "Different time series Prometheus currently holds. A sudden jump means some app sends many new labels.";
              targets = [(pq "sum(prometheus_tsdb_head_series{${sel}})" "series")];
              unit = "short";
            })
            (timeseries {
              title = "Metric storage on disk";
              description = "Space Prometheus uses: saved blocks, the write-ahead log and recent data.";
              targets = [
                (pq "sum(prometheus_tsdb_storage_blocks_bytes{${sel}})" "blocks")
                (pq "sum(prometheus_tsdb_wal_storage_size_bytes{${sel}})" "write-ahead log")
                (pq "sum(prometheus_tsdb_head_chunks_storage_size_bytes{${sel}})" "recent data")
              ];
              unit = "bytes";
              custom = {
                drawStyle = "line";
                lineWidth = 1;
                fillOpacity = 30;
                showPoints = "never";
                spanNulls = true;
                stacking = {
                  mode = "normal";
                  group = "A";
                };
              };
            })
            (timeseries {
              title = "Log streams in memory";
              description = "Different log streams (label combinations) Loki currently holds.";
              targets = [(pq "sum(loki_ingester_memory_streams{${sel}})" "streams")];
              unit = "short";
            })

            (row "Alerts" null)
            (bars {
              title = "Alert rule checks";
              description = "How often Grafana checked the alert rules, and how often a check failed (the rule could not be evaluated).";
              targets = [
                (pq "sum(increase(grafana_alerting_rule_evaluations_total{${sel}}[$__interval]))" "checks")
                (pq "sum(increase(grafana_alerting_rule_evaluation_failures_total{${sel}}[$__interval])) > 0" "failed")
              ];
            })
            (bars {
              title = "Alert messages";
              description = "Alert messages Grafana sent, and those that could not be delivered.";
              targets = [
                (pq "sum(increase(grafana_alerting_notifications_total{${sel}}[$__interval])) > 0" "sent")
                (pq "sum(increase(grafana_alerting_notifications_failed_total{${sel}}[$__interval])) > 0" "failed")
              ];
            })
            (timeseries {
              title = "Alerts by state";
              description = "Alert rules that are firing, waiting or in error right now.";
              targets = [(pq ''sum by (state) (grafana_alerting_alerts{${sel}, state=~"alerting|pending|error|nodata"})'' "{{state}}")];
              unit = "short";
            })

            (row "Containers" null)
            (timeseries {
              title = "Processor";
              targets = [(pq "sum by (name) (rate(container_cpu_usage_seconds_total{${containers}}[$__rate_interval]))" "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq "sum by (name) (container_memory_working_set_bytes{${containers}})" "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-(grafana|prometheus|loki)\\.service"} |~ "(?i)(error|warn|exception|fail)"'')];
            })
          ];
        };
      };
    };
  };
}
