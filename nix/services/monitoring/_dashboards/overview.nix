# Neo dashboard "Overview" (folder Neo). Not a module: imported by
# _grafana.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge bars dashboard fsFilter logs lq pos pq row stat stateTimeline thresholdsGreenRed timeseries;
in
  dashboard {
    uid = "neo-overview";
    title = "Overview";
    time = "now-24h";
    panels = [
      (stat {
        title = "Servers reporting";
        gridPos = pos 0 0 4 4;
        description = "Servers whose monitoring agent sent numbers in the last few minutes.";
        targets = [(pq "count(group by (host) (node_boot_time_seconds))" "")];
        thresholds = thresholdsGreenRed [];
        options.colorMode = "value";
      })
      (stat {
        title = "Uptime";
        gridPos = pos 4 0 4 4;
        description = "Share of uptime checks that succeeded in the selected time range.";
        targets = [(pq "100 * avg(avg_over_time(probe_success[$__range]))" "")];
        unit = "percent";
        decimals = 2;
        thresholds = {
          mode = "absolute";
          steps = [
            {
              color = "red";
              value = null;
            }
            {
              color = "orange";
              value = 99;
            }
            {
              color = "green";
              value = 99.9;
            }
          ];
        };
      })
      (stat {
        title = "Apps down";
        gridPos = pos 8 0 4 4;
        # An app is checked from its own server and from the Monitoring server: count it once.
        description = "Apps whose uptime check fails right now.";
        targets = [(pq "count(group by (target) (probe_success == 0)) or vector(0)" "")];
        thresholds = thresholdsGreenRed [
          {
            color = "red";
            value = 1;
          }
        ];
      })
      (stat {
        title = "Failed units";
        gridPos = pos 12 0 4 4;
        description = "Services and containers systemd reports as failed, on all servers.";
        targets = [(pq ''sum(node_systemd_unit_state{state="failed"}) or vector(0)'' "")];
        thresholds = thresholdsGreenRed [
          {
            color = "red";
            value = 1;
          }
        ];
      })
      (stat {
        title = "Failed logins";
        gridPos = pos 16 0 4 4;
        description = "Failed sign-ins (homeserver login and SSH) in the selected time range. Without SSH scans (user names that do not exist, keys-only server): those are under Logins.";
        targets = [(pq ''round(sum(increase(neo_auth_events_total{event="login",result="failure",attempt!="scan"}[$__range]))) or vector(0)'' "")];
        thresholds = thresholdsGreenRed [
          {
            color = "orange";
            value = 10;
          }
          {
            color = "red";
            value = 50;
          }
        ];
      })
      (stat {
        title = "Server errors";
        gridPos = pos 20 0 4 4;
        description = "Share of web requests that ended in a server error (5xx) in the selected time range.";
        targets = [(pq ''100 * (sum(increase(neo_http_requests_total{status_class="5xx"}[$__range])) or vector(0)) / sum(increase(neo_http_requests_total[$__range]))'' "")];
        unit = "percent";
        decimals = 2;
        thresholds = thresholdsGreenRed [
          {
            color = "orange";
            value = 1;
          }
          {
            color = "red";
            value = 5;
          }
        ];
      })

      (row "Servers" 4)
      (bargauge {
        title = "Processor";
        gridPos = pos 0 5 6 6;
        targets = [(pq ''100 * (1 - avg by (host) (rate(node_cpu_seconds_total{mode="idle"}[5m])))'' "{{host}}")];
        unit = "percent";
        min = 0;
        max = 100;
        thresholds = thresholdsGreenRed [
          {
            color = "orange";
            value = 70;
          }
          {
            color = "red";
            value = 90;
          }
        ];
      })
      (bargauge {
        title = "Memory";
        gridPos = pos 6 5 6 6;
        targets = [(pq "100 * (1 - node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes)" "{{host}}")];
        unit = "percent";
        min = 0;
        max = 100;
        thresholds = thresholdsGreenRed [
          {
            color = "orange";
            value = 80;
          }
          {
            color = "red";
            value = 95;
          }
        ];
      })
      (bargauge {
        title = "Fullest disk";
        gridPos = pos 12 5 6 6;
        targets = [(pq "max by (host) (100 * (1 - node_filesystem_avail_bytes{${fsFilter}} / node_filesystem_size_bytes{${fsFilter}}))" "{{host}}")];
        unit = "percent";
        min = 0;
        max = 100;
        thresholds = thresholdsGreenRed [
          {
            color = "orange";
            value = 80;
          }
          {
            color = "red";
            value = 90;
          }
        ];
      })
      (bargauge {
        title = "Running since";
        gridPos = pos 18 5 6 6;
        targets = [(pq "node_time_seconds - node_boot_time_seconds" "{{host}}")];
        unit = "s";
        options.displayMode = "basic";
        thresholds = {
          mode = "absolute";
          steps = [
            {
              color = "blue";
              value = null;
            }
          ];
        };
      })

      (row "Uptime and app health" 11)
      # Only apps that were down at some point in the range: a row per app
      # does not fit once a server runs a few dozen of them.
      (stateTimeline {
        title = "Uptime checks with problems";
        gridPos = pos 0 12 12 10;
        description = "Web addresses that did not answer at some point in the selected time, checked every 30 seconds by the monitoring agents. Empty when every app answered the whole time.";
        targets = [(pq ''label_replace(probe_success and on (target) (min_over_time(probe_success[$__range]) < 1), "app", "$1", "target", "https?://([^/]+)/?.*")'' "{{app}}")];
        noValue = "Every app answered the whole time";
      })
      (stateTimeline {
        title = "Apps with stopped containers";
        gridPos = pos 12 12 12 10;
        description = "Apps where a container was not running at some point in the selected time. Empty when all of them ran the whole time.";
        targets = [
          (pq ''
            min by (host, service) (neo_service_unit{kind="container"} * on (host, name) group_left () node_systemd_unit_state{state="active"})
            and on (host, service)
            (min_over_time((min by (host, service) (neo_service_unit{kind="container"} * on (host, name) group_left () node_systemd_unit_state{state="active"}))[$__range:1m]) < 1)
          '' "{{service}} ({{host}})")
        ];
        noValue = "All apps ran the whole time";
      })
      (bargauge {
        title = "Uptime per app";
        gridPos = pos 0 22 12 8;
        targets = [(pq "100 * avg by (target) (avg_over_time(probe_success[$__range]))" "{{target}}")];
        unit = "percent";
        decimals = 2;
        min = 90;
        max = 100;
        thresholds = {
          mode = "absolute";
          steps = [
            {
              color = "red";
              value = null;
            }
            {
              color = "orange";
              value = 99;
            }
            {
              color = "green";
              value = 99.9;
            }
          ];
        };
      })
      (timeseries {
        title = "Response time";
        gridPos = pos 12 22 12 8;
        targets = [(pq "probe_duration_seconds" "{{target}} from {{host}}")];
        unit = "s";
      })

      (row "Logins" 30)
      (bars {
        title = "Sign-ins";
        gridPos = pos 0 31 12 8;
        description = "Homeserver login (tinyauth) and SSH sign-ins. ssh scan: attempts with a user name that does not exist on a server that accepts keys only (Internet scanners); they cannot succeed.";
        # attempt="scan" (monitoring-agent/_alloy.nix) shown as its own result.
        targets = [(pq ''sum by (result, provider) (label_replace(increase(neo_auth_events_total{event="login"}[$__interval]), "result", "scan", "attempt", "scan"))'' "{{provider}} {{result}}")];
        overrides = [
          {
            matcher = {
              id = "byRegexp";
              options = ".*failure";
            };
            properties = [
              {
                id = "color";
                value = {
                  mode = "fixed";
                  fixedColor = "red";
                };
              }
            ];
          }
          {
            matcher = {
              id = "byRegexp";
              options = ".*scan";
            };
            properties = [
              {
                id = "color";
                value = {
                  mode = "fixed";
                  fixedColor = "orange";
                };
              }
            ];
          }
        ];
      })
      (logs {
        title = "Recent sign-ins";
        gridPos = pos 12 31 12 8;
        targets = [(lq ''{event=~"login|logout"}'')];
      })

      (row "Web traffic" 39)
      (timeseries {
        title = "Requests per app";
        gridPos = pos 0 40 8 8;
        targets = [(pq "sum by (service) (rate(neo_http_requests_total[$__rate_interval]))" "{{service}}")];
        unit = "reqps";
      })
      (timeseries {
        title = "Errors per app";
        gridPos = pos 8 40 8 8;
        description = "Server errors (5xx) per second.";
        targets = [(pq ''sum by (service) (rate(neo_http_requests_total{status_class="5xx"}[$__rate_interval]))'' "{{service}}")];
        unit = "reqps";
      })
      (timeseries {
        title = "Slowest answers (95%)";
        gridPos = pos 16 40 8 8;
        targets = [(pq "histogram_quantile(0.95, sum by (service, le) (rate(neo_http_request_duration_seconds_bucket[$__rate_interval])))" "{{service}}")];
        unit = "s";
      })
    ];
  }
