# ISO 27001 dashboard (folder "ISO 27001"). Not a module: imported by
# _iso27001.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge daysLeft fsFilter iso orange pos pq red text thresholdsGreenRed timeseries;
in
  iso {
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
  }
