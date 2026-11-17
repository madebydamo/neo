# ISO 27001 dashboard (folder "ISO 27001"). Not a module: imported by
# _iso27001.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge bars iso logs loki lq lqLegend orange pos pq red stateTimeline switchLines text thresholdsGreenRed timerAgeHours;
in
  iso {
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
  }
