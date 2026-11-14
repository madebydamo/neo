# ISO/IEC 27001:2022 evidence dashboards and alert rules (folder "ISO 27001").
# Not a module: imported by _grafana.nix with its panel helpers (`h`).
#
# They collect technical evidence for Annex A controls that monitoring can
# show. They do not make anyone compliant: the ISMS (scope, risk assessment,
# Statement of Applicability, policies, reviews) stays an organisational task.
#
# Data beyond the Neo dashboards (monitoring-agent/_alloy.nix):
#   sudo commands as {event="privileged", provider="sudo"} (username, target,
#   command as structured metadata) and neo_auth_events_total{event="privileged"};
#   node_systemd_timer_last_trigger_seconds (timers), node_timex_* (clock);
#   loki_write_* / prometheus_remote_storage_* (agent delivery).
{
  lib,
  h,
}: let
  inherit (h) pq pqInstant lq lqLegend pos thresholdsGreenRed stat timeseries bars bargauge logs stateTimeline table text dashboard fsFilter rule loki;
  folder = "neo-iso27001";

  iso = args:
    dashboard args
    // {
      tags = ["neo" "iso27001"];
      links = [
        {
          title = "ISO 27001";
          type = "dashboards";
          tags = ["iso27001"];
          asDropdown = true;
          includeVars = false;
          keepTime = true;
        }
      ];
    };

  red = v: {
    color = "red";
    value = v;
  };
  orange = v: {
    color = "orange";
    value = v;
  };

  instantLoki = expr: legend: (lqLegend expr legend) // {queryType = "instant";};

  # Units a person ran sudo from (SSH or console session), not services.
  interactive = ''unit="session.scope"'';

  switchLines = ''{job="journal", unit="nixos-rebuild-switch-to-configuration.service"} |= "switching to system configuration"'';

  # Timers that never ran report 0: left out.
  timerAgeHours = names: "(time() - (node_systemd_timer_last_trigger_seconds{name=~\"${names}\"} > 0)) / 3600";

  overview = iso {
    uid = "neo-iso-overview";
    title = "ISO 27001: Evidence overview";
    time = "now-30d";
    refresh = "5m";
    panels = [
      (text {
        gridPos = pos 0 0 24 7;
        content = ''
          Technical evidence for ISO/IEC 27001:2022 Annex A controls, collected from every server with the monitoring agent.
          It supports audits and the monitoring the controls ask for; it does not replace the ISMS (scope, risk assessment, Statement of Applicability, policies, management review).
          Set the time range to the audit period. Each dashboard below names the controls it covers.

          | Control | Dashboard |
          |---|---|
          | A.8.15 Logging · A.8.16 Monitoring activities · A.8.17 Clock synchronisation · A.5.28 Collection of evidence | [Logging and monitoring](/d/neo-iso-logging) |
          | A.5.15–A.5.18 Access control and identities · A.8.2 Privileged access · A.8.5 Secure authentication | [Access and authentication](/d/neo-iso-access) |
          | A.8.8 Technical vulnerabilities · A.8.9 Configuration · A.8.13 Backup · A.8.32 Change management | [Changes, updates and backup](/d/neo-iso-changes) |
          | A.8.6 Capacity · A.8.14 Redundancy · A.5.30 ICT readiness for business continuity | [Capacity and availability](/d/neo-iso-capacity) |
          | A.8.20 Network security · A.8.16 Anomalies · A.5.7 Threat intelligence · A.5.24–A.5.26 Incidents | [Network and threats](/d/neo-iso-network) |
        '';
      })
      (stat {
        title = "A.8.16 Servers reporting";
        gridPos = pos 0 7 4 4;
        targets = [(pq "count(group by (host) (node_boot_time_seconds))" "")];
        options.colorMode = "value";
        thresholds = thresholdsGreenRed [];
      })
      (stat {
        title = "A.8.15 Servers sending logs";
        gridPos = pos 4 7 4 4;
        description = "Servers whose logs arrived in the last 15 minutes.";
        datasource = loki;
        targets = [(instantLoki ''count(sum by (host) (count_over_time({job="journal"}[15m])))'' "")];
        options.colorMode = "value";
        thresholds = thresholdsGreenRed [];
      })
      (stat {
        title = "A.8.17 Clocks in sync";
        gridPos = pos 8 7 4 4;
        description = "Servers whose clock is synchronised, of all servers reporting.";
        targets = [(pq "sum(node_timex_sync_status) / count(node_timex_sync_status)" "")];
        unit = "percentunit";
        thresholds = {
          mode = "absolute";
          steps = [
            (red null)
            (orange 0.5)
            {
              color = "green";
              value = 1;
            }
          ];
        };
      })
      (stat {
        title = "A.8.13 Hours since last backup";
        gridPos = pos 12 7 4 4;
        targets = [(pq "min(${timerAgeHours "backup.timer"})" "")];
        unit = "h";
        decimals = 1;
        thresholds = thresholdsGreenRed [(orange 26) (red 50)];
      })
      (stat {
        title = "A.8.8 Days since last system update";
        gridPos = pos 16 7 4 4;
        targets = [(pq ''max(${timerAgeHours "neo-auto-update.timer"}) / 24'' "")];
        # systemd forgets the last run of a timer at a restart.
        noValue = "None since restart";
        unit = "d";
        decimals = 1;
        thresholds = thresholdsGreenRed [(orange 8) (red 31)];
      })
      (stat {
        title = "A.8.32 System changes";
        gridPos = pos 20 7 4 4;
        description = "Configuration switches (NixOS generations) in the selected period.";
        datasource = loki;
        targets = [(instantLoki "sum(count_over_time(${switchLines} [$__range]))" "")];
        options.colorMode = "value";
        thresholds = thresholdsGreenRed [];
      })
      (stat {
        title = "A.8.5 Failed sign-ins";
        gridPos = pos 0 11 4 4;
        targets = [(pq ''round(sum(increase(neo_auth_events_total{event="login",result="failure"}[$__range]))) or vector(0)'' "")];
        thresholds = thresholdsGreenRed [(orange 20) (red 100)];
      })
      (stat {
        title = "A.8.2 Privileged commands by people";
        gridPos = pos 4 11 4 4;
        description = "sudo run from sign-in sessions (SSH, console), not by services.";
        datasource = loki;
        targets = [(instantLoki ''sum(count_over_time({event="privileged", ${interactive}} [$__range]))'' "")];
        options.colorMode = "value";
        thresholds = thresholdsGreenRed [];
      })
      (stat {
        title = "A.8.14 Uptime";
        gridPos = pos 8 11 4 4;
        targets = [(pq "100 * avg(avg_over_time(probe_success[$__range]))" "")];
        unit = "percent";
        decimals = 3;
        thresholds = {
          mode = "absolute";
          steps = [
            (red null)
            (orange 99)
            {
              color = "green";
              value = 99.9;
            }
          ];
        };
      })
      (stat {
        title = "A.8.6 Fullest disk";
        gridPos = pos 12 11 4 4;
        targets = [(pq "max(100 * (1 - node_filesystem_avail_bytes{${fsFilter}} / node_filesystem_size_bytes{${fsFilter}}))" "")];
        unit = "percent";
        thresholds = thresholdsGreenRed [(orange 80) (red 90)];
      })
      (stat {
        title = "A.8.15 Log lines dropped";
        gridPos = pos 16 11 4 4;
        description = "Log lines an agent could not deliver in the selected period.";
        targets = [(pq "round(sum(increase(loki_write_dropped_entries_total[$__range]))) or vector(0)" "")];
        thresholds = thresholdsGreenRed [(red 1)];
      })
      (h.panel {
        title = "A.5.25 Firing alerts";
        type = "alertlist";
        gridPos = pos 20 11 4 4;
        targets = [];
        options = {
          viewMode = "stat";
          stateFilter = {
            firing = true;
            pending = false;
            noData = false;
            normal = false;
            error = true;
          };
          maxItems = 20;
        };
      })
    ];
  };

  logging = iso {
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
  };

  access = iso {
    uid = "neo-iso-access";
    title = "ISO 27001: Access and authentication";
    time = "now-30d";
    panels = [
      (text {
        gridPos = pos 0 0 24 3;
        content = ''
          **A.5.15 Access control, A.5.16 Identity management, A.5.17 Authentication information, A.5.18 Access rights, A.8.2 Privileged access rights, A.8.5 Secure authentication.**
          Sign-ins to the homeserver login (tinyauth) and SSH, and every sudo command. User names and addresses are stored with each event.
        '';
      })
      (bars {
        title = "Sign-ins";
        gridPos = pos 0 3 24 7;
        targets = [(pq ''sum by (provider, result) (increase(neo_auth_events_total{event="login"}[$__interval]))'' "{{provider}} {{result}}")];
      })
      (table {
        title = "Who signed in";
        gridPos = pos 0 10 12 9;
        description = "Successful sign-ins per user and method in the selected period. Review against the list of people who should have access (A.5.18).";
        datasource = loki;
        targets = [((instantLoki ''sum by (username, provider, host) (count_over_time({event="login", result="success"} [$__range]))'' "") // {format = "table";})];
        transformations = [
          {
            id = "organize";
            options = {
              excludeByName.Time = true;
              renameByName = {
                username = "User";
                provider = "Method";
                host = "Server";
                Value = "Sign-ins";
              };
            };
          }
        ];
      })
      (table {
        title = "Failed sign-ins";
        gridPos = pos 12 10 12 9;
        datasource = loki;
        targets = [((instantLoki ''sum by (username, ip, reason, provider) (count_over_time({event="login", result="failure"} [$__range]))'' "") // {format = "table";})];
        transformations = [
          {
            id = "organize";
            options = {
              excludeByName.Time = true;
              renameByName = {
                username = "User";
                ip = "Address";
                reason = "Reason";
                provider = "Method";
                Value = "Attempts";
              };
            };
          }
          {
            id = "sortBy";
            options.sort = [
              {
                field = "Attempts";
                desc = true;
              }
            ];
          }
        ];
      })
      (logs {
        title = "Locked accounts";
        gridPos = pos 0 19 12 8;
        targets = [(lq ''{event="login", result="failure"} | reason=~".*locked.*"'')];
      })
      (logs {
        title = "SSH sign-ins as root";
        gridPos = pos 12 19 12 8;
        targets = [(lq ''{event="login", provider="ssh"} | username="root"'')];
      })
      (bargauge {
        title = "Privileged commands by people";
        gridPos = pos 0 27 8 9;
        description = "sudo from sign-in sessions, per user.";
        datasource = loki;
        targets = [(instantLoki ''sum by (username, target) (count_over_time({event="privileged", ${interactive}} [$__range]))'' "{{username}} → {{target}}")];
        thresholds = thresholdsGreenRed [];
      })
      (logs {
        title = "Privileged commands by people";
        gridPos = pos 8 27 16 9;
        targets = [(lq ''{event="privileged", ${interactive}}'')];
      })
      (bargauge {
        title = "Privileged commands by services";
        gridPos = pos 0 36 8 9;
        description = "sudo used by Neo's own services (updates, backups, the web UI). Expected; review for unexpected units.";
        datasource = loki;
        targets = [(instantLoki ''sum by (unit) (count_over_time({event="privileged", unit!="session.scope"} [$__range]))'' "{{unit}}")];
        thresholds = thresholdsGreenRed [];
      })
      (logs {
        title = "Refused privileged commands";
        gridPos = pos 8 36 16 9;
        targets = [(lq ''{event="privileged", result="failure"}'')];
      })
    ];
  };

  changes = iso {
    uid = "neo-iso-changes";
    title = "ISO 27001: Changes, updates and backup";
    time = "now-30d";
    panels = [
      (text {
        gridPos = pos 0 0 24 3;
        content = ''
          **A.8.9 Configuration management, A.8.32 Change management, A.8.8 Management of technical vulnerabilities, A.8.13 Information backup.**
          Every server is built from its versioned Neo configuration; each switch below is a new, revertible generation. Updates and backups run on timers.
        '';
      })
      (bars {
        title = "System changes";
        gridPos = pos 0 3 12 7;
        description = "Configuration switches (new NixOS generation activated).";
        datasource = loki;
        targets = [(lqLegend "sum by (host) (count_over_time(${switchLines} [$__auto]))" "{{host}}")];
      })
      (bars {
        title = "Restarts";
        gridPos = pos 12 3 12 7;
        targets = [(pq "changes(node_boot_time_seconds[$__interval]) > 0" "{{host}}")];
      })
      (logs {
        title = "System changes";
        gridPos = pos 0 10 24 7;
        targets = [(lq switchLines)];
      })
      (bargauge {
        title = "Hours since each job last ran";
        gridPos = pos 0 17 12 9;
        description = "Backups, system and app updates, security scans.";
        targets = [(pq (timerAgeHours "backup.timer|neo-auto-update.timer|neo-docker-updater.timer|neo-security-scan.timer") "{{host}} {{name}}")];
        unit = "h";
        decimals = 1;
        thresholds = thresholdsGreenRed [(orange 26) (red 170)];
      })
      (stateTimeline {
        title = "Jobs that failed";
        gridPos = pos 12 17 12 9;
        description = "Red while the last run of the job failed.";
        targets = [(pq ''1 - node_systemd_unit_state{name=~"backup.service|neo-auto-update.service|neo-docker-updater.service|neo-security-scan.service",state="failed"}'' "{{host}} {{name}}")];
        mappings = [
          {
            type = "value";
            options = {
              "0" = {
                text = "Failed";
                color = "red";
                index = 0;
              };
              "1" = {
                text = "OK";
                color = "green";
                index = 1;
              };
            };
          }
        ];
      })
      (bargauge {
        title = "Container starts";
        gridPos = pos 0 26 12 9;
        description = "How often each container was (re)started, e.g. after an image update.";
        targets = [(pq ''sum by (host, name) (changes(container_start_time_seconds{name!=""}[$__range]))'' "{{host}} {{name}}")];
        thresholds = thresholdsGreenRed [(orange 10)];
      })
      (logs {
        title = "Backup runs";
        gridPos = pos 12 26 12 9;
        targets = [(lq ''{job="journal", unit="backup.service"}'')];
      })
      (logs {
        title = "Updates";
        gridPos = pos 0 35 12 10;
        targets = [(lq ''{job="journal", unit=~"neo-auto-update.service|neo-docker-updater.service"}'')];
      })
      (logs {
        title = "Security scans";
        gridPos = pos 12 35 12 10;
        targets = [(lq ''{job="journal", unit="neo-security-scan.service"}'')];
      })
    ];
  };

  daysLeft = ''node_filesystem_avail_bytes{${fsFilter}} / clamp_min(-deriv(node_filesystem_avail_bytes{${fsFilter}}[1d]), 0.001) / 86400'';

  capacity = iso {
    uid = "neo-iso-capacity";
    title = "ISO 27001: Capacity and availability";
    time = "now-30d";
    panels = [
      (text {
        gridPos = pos 0 0 24 3;
        content = ''
          **A.8.6 Capacity management, A.8.14 Redundancy of information processing facilities, A.5.30 ICT readiness for business continuity.**
          Availability measured from outside by the uptime checks, and resource trends with a forecast for disks.
        '';
      })
      (bargauge {
        title = "Availability per app";
        gridPos = pos 0 3 12 9;
        targets = [(pq "100 * avg by (target) (avg_over_time(probe_success[$__range]))" "{{target}}")];
        unit = "percent";
        decimals = 3;
        min = 95;
        max = 100;
        thresholds = {
          mode = "absolute";
          steps = [
            (red null)
            (orange 99)
            {
              color = "green";
              value = 99.9;
            }
          ];
        };
      })
      (bargauge {
        title = "Downtime per app";
        gridPos = pos 12 3 12 9;
        targets = [(pq "(1 - avg by (target) (avg_over_time(probe_success[$__range]))) * $__range_s / 60" "{{target}}")];
        unit = "m";
        decimals = 0;
        thresholds = thresholdsGreenRed [(orange 30) (red 240)];
      })
      (bargauge {
        title = "Days until disk full";
        gridPos = pos 0 12 12 9;
        description = "At the growth of the last day. Large values mean no growth.";
        targets = [(pq "clamp_max(${daysLeft}, 999)" "{{host}} {{mountpoint}}")];
        unit = "d";
        decimals = 0;
        min = 0;
        max = 365;
        thresholds = {
          mode = "absolute";
          steps = [
            (red null)
            (orange 30)
            {
              color = "green";
              value = 90;
            }
          ];
        };
      })
      (timeseries {
        title = "Disk use";
        gridPos = pos 12 12 12 9;
        targets = [(pq "100 * (1 - node_filesystem_avail_bytes{${fsFilter}} / node_filesystem_size_bytes{${fsFilter}})" "{{host}} {{mountpoint}}")];
        unit = "percent";
        min = 0;
        max = 100;
      })
      (timeseries {
        title = "Processor (daily peak)";
        gridPos = pos 0 21 12 8;
        targets = [(pq ''max_over_time((100 * (1 - avg by (host) (rate(node_cpu_seconds_total{mode="idle"}[5m]))))[1d:5m])'' "{{host}}")];
        unit = "percent";
        min = 0;
        max = 100;
      })
      (timeseries {
        title = "Memory (daily peak)";
        gridPos = pos 12 21 12 8;
        targets = [(pq "max_over_time((100 * (1 - node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes))[1d:5m])" "{{host}}")];
        unit = "percent";
        min = 0;
        max = 100;
      })
    ];
  };

  network = iso {
    uid = "neo-iso-network";
    title = "ISO 27001: Network and threats";
    time = "now-7d";
    panels = [
      (text {
        gridPos = pos 0 0 24 3;
        content = ''
          **A.8.20 Network security, A.8.16 Monitoring activities (anomalies), A.5.7 Threat intelligence, A.5.24–A.5.26 Incident management.**
          All web traffic enters through SWAG. Refused requests, scans and automatic bans show attempts against the servers.
          With CrowdSec on: [CrowdSec](/d/app-swag-crowdsec) shows active bans, detected attacks and the community blocklist.
        '';
      })
      (bars {
        title = "Web requests by result";
        gridPos = pos 0 3 24 7;
        targets = [(pq "sum by (status_class) (increase(neo_http_requests_total[$__interval]))" "{{status_class}}")];
      })
      (bargauge {
        title = "Refused requests per address";
        gridPos = pos 0 10 8 10;
        datasource = loki;
        targets = [(instantLoki ''topk(15, sum by (ip) (count_over_time({job="swag", status_class="4xx"} [$__range])))'' "{{ip}}")];
        thresholds = thresholdsGreenRed [(orange 100) (red 1000)];
      })
      (bargauge {
        title = "Most probed missing paths";
        gridPos = pos 8 10 8 10;
        description = "Paths answered with 404, typical for scanners.";
        datasource = loki;
        targets = [(instantLoki ''topk(15, sum by (path) (count_over_time({job="swag"} | status="404" [$__range])))'' "{{path}}")];
        thresholds = thresholdsGreenRed [];
      })
      (bargauge {
        title = "Requests for unknown host names";
        gridPos = pos 16 10 8 10;
        description = "Requests that name no app on the server (scanners trying addresses).";
        datasource = loki;
        targets = [(instantLoki ''topk(15, sum by (vhost) (count_over_time({job="swag", service="other"} [$__range])))'' "{{vhost}}")];
        thresholds = thresholdsGreenRed [];
      })
      (timeseries {
        title = "Server errors per app";
        gridPos = pos 0 20 12 8;
        targets = [(pq ''sum by (service) (rate(neo_http_requests_total{status_class="5xx"}[$__rate_interval]))'' "{{service}}")];
        unit = "reqps";
      })
      (logs {
        title = "Automatic bans (CrowdSec, fail2ban)";
        gridPos = pos 12 20 12 8;
        targets = [(lq ''{job="journal", unit=~"docker-crowdsec.service|docker-swag.service"} |~ "(?i)(ban|decision)"'')];
      })
      (h.panel {
        title = "Alerts (A.5.25 assessment of events)";
        type = "alertlist";
        gridPos = pos 0 28 24 8;
        targets = [];
        options = {
          viewMode = "list";
          showInstances = true;
          stateFilter = {
            firing = true;
            pending = true;
            noData = false;
            normal = false;
            error = true;
          };
          maxItems = 50;
          sortOrder = 1;
        };
      })
    ];
  };

  isoRule = args:
    rule ({
        inherit folder;
        group = "iso27001";
      }
      // args);
in {
  folders.${folder}.title = "ISO 27001";

  dashboards = lib.mapAttrs (_: dashboard: {inherit folder dashboard;}) {
    iso-overview = overview;
    iso-logging = logging;
    iso-access = access;
    iso-changes = changes;
    iso-capacity = capacity;
    iso-network = network;
  };

  alertRules = [
    (isoRule {
      uid = "neo-iso-clock";
      title = "Clock not synchronised";
      expr = "1 - node_timex_sync_status";
      for = "30m";
      summary = "The clock of {{ $labels.host }} has not been synchronised for 30 minutes; log times may be wrong (A.8.17).";
    })
    (isoRule {
      uid = "neo-iso-backup-overdue";
      title = "Backup overdue";
      expr = timerAgeHours "backup.timer";
      above = 26;
      for = "0s";
      summary = "The last backup on {{ $labels.host }} started {{ printf \"%.0f\" $values.B.Value }} hours ago (A.8.13).";
    })
    (isoRule {
      uid = "neo-iso-logs-dropped";
      title = "Logs are being lost";
      expr = "sum by (host) (increase(loki_write_dropped_entries_total[15m]))";
      for = "0s";
      summary = "{{ $labels.host }} could not deliver {{ printf \"%.0f\" $values.B.Value }} log lines in 15 minutes (A.8.15).";
    })
    (isoRule {
      uid = "neo-iso-logs-silent";
      title = "No logs from a server";
      expr = "sum by (host) (increase(loki_write_sent_entries_total[15m])) == bool 0";
      for = "15m";
      summary = "{{ $labels.host }} has sent no log lines for 30 minutes (A.8.15).";
    })
  ];
}
