# ISO 27001 dashboard (folder "ISO 27001"). Not a module: imported by
# _iso27001.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge bars instantLoki iso logs loki lq lqLegend pos pq red stateTimeline text timeseries;
in
  iso {
    uid = "neo-iso-logging";
    title = "ISO 27001: Logging and monitoring";
    time = "now-7d";
    panels = [
      (text {
        gridPos = pos 0 0 24 3;
        content = ''
          **A.8.15 Logging, A.8.16 Monitoring activities, A.8.17 Clock synchronisation, A.5.28 Collection of evidence.**
          Every server's system log (all services and containers), web requests and sign-ins are stored centrally; retention is set in Neo under Monitoring. Logs cannot be edited through Grafana.
        '';
      })
      (bars {
        title = "Log lines per server";
        gridPos = pos 0 3 12 8;
        datasource = loki;
        targets = [(lqLegend ''sum by (host) (count_over_time({job=~"journal|swag"} [$__auto]))'' "{{host}}")];
      })
      (bargauge {
        title = "Logs in the last 15 minutes";
        gridPos = pos 12 3 12 8;
        description = "A server missing here sends no logs.";
        datasource = loki;
        targets = [(instantLoki ''sum by (host, job) (count_over_time({job=~"journal|swag"}[15m]))'' "{{host}} {{job}}")];
        thresholds = {
          mode = "absolute";
          steps = [
            (red null)
            {
              color = "green";
              value = 1;
            }
          ];
        };
      })
      (timeseries {
        title = "Agent delivery: log lines";
        gridPos = pos 0 11 12 8;
        targets = [
          (pq "sum by (host) (rate(loki_write_sent_entries_total[$__rate_interval]))" "{{host}} sent")
          (pq "sum by (host) (rate(loki_write_dropped_entries_total[$__rate_interval]))" "{{host}} dropped")
        ];
        unit = "short";
      })
      (timeseries {
        title = "Agent delivery: numbers";
        gridPos = pos 12 11 12 8;
        targets = [
          (pq "sum by (host) (rate(prometheus_remote_storage_samples_total[$__rate_interval]))" "{{host}} sent")
          (pq "sum by (host) (rate(prometheus_remote_storage_samples_failed_total[$__rate_interval]))" "{{host}} failed")
        ];
        unit = "short";
      })
      (stateTimeline {
        title = "Clock synchronised";
        gridPos = pos 0 19 12 7;
        targets = [(pq "node_timex_sync_status" "{{host}}")];
        mappings = [
          {
            type = "value";
            options = {
              "0" = {
                text = "Not synced";
                color = "red";
                index = 0;
              };
              "1" = {
                text = "Synced";
                color = "green";
                index = 1;
              };
            };
          }
        ];
      })
      (timeseries {
        title = "Clock offset";
        gridPos = pos 12 19 12 7;
        targets = [(pq "node_timex_offset_seconds" "{{host}}")];
        unit = "s";
      })
      (stateTimeline {
        title = "Monitoring itself";
        gridPos = pos 0 26 24 6;
        description = "Containers of Monitoring and the monitoring agents.";
        targets = [(pq ''min by (host, service) (neo_service_unit{service=~"monitoring|monitoring-agent",kind="container"} * on (host, name) group_left () node_systemd_unit_state{state="active"})'' "{{service}} ({{host}})")];
      })
      (logs {
        title = "Errors and warnings, all servers";
        gridPos = pos 0 32 24 12;
        targets = [(lq ''{job="journal", level=~"emerg|alert|crit|err|warning"}'')];
      })
    ];
  }
