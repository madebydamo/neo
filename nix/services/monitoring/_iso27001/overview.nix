# ISO 27001 dashboard (folder "ISO 27001"). Not a module: imported by
# _iso27001.nix with the panel helpers (`g`).
{g}: let
  inherit (g) fsFilter instantLoki interactive iso loki orange pos pq red stat switchLines text thresholdsGreenRed timerAgeHours;
in
  iso {
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
        description = "Failed sign-ins of existing users and to the homeserver login. SSH scans (user names that do not exist, keys-only server) are counted apart on the Access page.";
        targets = [(pq ''round(sum(increase(neo_auth_events_total{event="login",result="failure",attempt!="scan"}[$__range]))) or vector(0)'' "")];
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
      (g.panel {
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
  }
