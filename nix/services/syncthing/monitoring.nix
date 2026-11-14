# Syncthing metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# Syncthing always serves Prometheus metrics on the GUI port (:8384/metrics).
# Without a GUI user/password (Neo sets none: tinyauth guards the subdomain and
# syncthing-config only turns on insecureAdminAccess) the path needs no auth.
# No API key on purpose: any Syncthing API key is full REST admin, and SWAG's
# /rest location skips tinyauth. The GUI host check only applies when the GUI
# listens on localhost; the linuxserver image binds 0.0.0.0:8384, so the
# agent's Host header (syncthing:8384) is accepted without extra headers.
#
# Metric names checked against lib/model, lib/protocol, lib/connections and
# lib/scanner metrics.go (v2.1.x). Folders are labelled by folder ID, devices by
# full device ID; the dashboard shortens device IDs to the 7-character short ID
# the Syncthing GUI shows.
{...}: {
  flake.modules.nixos.syncthing-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.syncthing;
    sel = lib.neo.grafana.appSelector "syncthing";

    shortDevice = q: ''label_replace(${q}, "device", "$1", "device", "([A-Z0-9]{7}).*")'';

    # lib/model/folderstate.go: FolderIdle = iota, …
    folderStates = [
      ["Idle" "green"]
      ["Scanning" "blue"]
      ["Waiting to scan" "light-blue"]
      ["Waiting to sync" "light-purple"]
      ["Preparing to sync" "purple"]
      ["Syncing" "dark-purple"]
      ["Cleaning old versions" "yellow"]
      ["Waiting to clean" "light-yellow"]
      ["Error" "red"]
      ["Starting" "text"]
    ];
    folderStateMappings = [
      {
        type = "value";
        options = lib.listToAttrs (lib.imap0 (i: s: {
            name = toString i;
            value = {
              text = builtins.elemAt s 0;
              color = builtins.elemAt s 1;
              index = i;
            };
          })
          folderStates);
      }
    ];
  in {
    config = lib.mkIf cfg.enabled {
      neo.monitoring.apps.syncthing = {
        hidePaths = ["/metrics"];

        metrics.syncthing = {
          container = "syncthing";
          port = 8384;
        };

        dashboards.overview = {
          title = "Syncthing";
          description = "Folder sync state, out-of-sync files, conflicts and the devices Syncthing talks to.";
          time = "now-24h";
          panels = with lib.neo.grafana; [
            (row "Overview" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "Syncthing answers.";
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
              title = "Folders";
              w = 4;
              h = 4;
              description = "Folders shared on this server.";
              targets = [(pq "count(syncthing_model_folder_state{${sel}}) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Devices connected";
              w = 4;
              h = 4;
              description = "Other devices connected right now.";
              targets = [(pq "count(sum by (device) (syncthing_connections_active{${sel}}) > 0) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Out of sync";
              w = 4;
              h = 4;
              description = "Files and folders this server still has to fetch from other devices.";
              targets = [(pq ''sum(syncthing_model_folder_summary{${sel}, scope="need", type=~"files|directories|symlinks|deleted"}) or vector(0)'' "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Folders with errors";
              w = 4;
              h = 4;
              description = "Folders that stopped syncing because of an error (for example a missing folder or a full disk).";
              targets = [(pq "count(syncthing_model_folder_state{${sel}} == 8) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "New conflicts";
              w = 4;
              h = 4;
              description = "Files changed on two devices at once in the selected time range. Syncthing keeps both, the other as a .sync-conflict copy.";
              targets = [(pq "round(sum(increase(syncthing_model_folder_conflicts_total{${sel}}[$__range]))) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 1;
                }
              ];
            })
            (stateTimeline {
              title = "Folder state";
              w = 24;
              h = 8;
              description = "What each folder was doing over time. Red means the folder stopped syncing.";
              targets = [(pq "max by (folder) (syncthing_model_folder_state{${sel}})" "{{folder}}")];
              mappings = folderStateMappings;
              thresholds = thresholdsGreenRed [];
            })

            (row "Folders" null)
            (bargauge {
              title = "Files per folder";
              description = "Files in each folder, as all devices together see it.";
              targets = [(pq ''sort_desc(sum by (folder) (syncthing_model_folder_summary{${sel}, scope="global", type="files"}))'' "{{folder}}")];
              thresholds = thresholdsGreenRed [];
            })
            (bargauge {
              title = "Size per folder";
              description = "Size of each folder, as all devices together see it.";
              targets = [(pq ''sort_desc(sum by (folder) (syncthing_model_folder_summary{${sel}, scope="global", type="bytes"}))'' "{{folder}}")];
              unit = "bytes";
              thresholds = thresholdsGreenRed [];
            })
            (timeseries {
              title = "Out of sync items";
              description = "Files and folders each folder still has to fetch.";
              targets = [(pq ''sum by (folder) (syncthing_model_folder_summary{${sel}, scope="need", type=~"files|directories|symlinks|deleted"})'' "{{folder}}")];
            })
            (timeseries {
              title = "Out of sync data";
              description = "Data each folder still has to fetch.";
              targets = [(pq ''sum by (folder) (syncthing_model_folder_summary{${sel}, scope="need", type="bytes"})'' "{{folder}}")];
              unit = "bytes";
            })
            (bars {
              title = "Conflicts";
              description = "Files changed on two devices at once, per folder.";
              targets = [(pq "sum by (folder) (increase(syncthing_model_folder_conflicts_total{${sel}}[$__interval]))" "{{folder}}")];
            })
            (bars {
              title = "Data synced";
              description = "Where the data for changed files came from: other devices, or blocks this server already had.";
              targets = [
                (pq ''sum(increase(syncthing_model_folder_processed_bytes_total{${sel}, source="network"}[$__interval]))'' "from other devices")
                (pq ''sum(increase(syncthing_model_folder_processed_bytes_total{${sel}, source=~"local_origin|local_other"}[$__interval]))'' "reused from local files")
                (pq ''sum(increase(syncthing_model_folder_processed_bytes_total{${sel}, source="skipped"}[$__interval]))'' "empty blocks")
              ];
              unit = "bytes";
            })
            (timeseries {
              title = "Time spent scanning";
              description = "Share of time each folder spends looking for local changes.";
              targets = [(pq "sum by (folder) (rate(syncthing_model_folder_scan_seconds_total{${sel}}[$__rate_interval]))" "{{folder}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Time spent syncing";
              description = "Share of time each folder spends fetching changes from other devices.";
              targets = [(pq "sum by (folder) (rate(syncthing_model_folder_pull_seconds_total{${sel}}[$__rate_interval]))" "{{folder}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Data read for changes";
              description = "Data hashed while scanning, per folder. High values mean many local changes.";
              targets = [(pq "sum by (folder) (rate(syncthing_scanner_hashed_bytes_total{${sel}}[$__rate_interval]))" "{{folder}}")];
              unit = "Bps";
            })

            (row "Devices" null)
            (stateTimeline {
              title = "Connected devices";
              w = 24;
              h = 6;
              description = "When each device was connected. Devices are shown by their short ID, as in the Syncthing page.";
              targets = [(pq (shortDevice "clamp_max(sum by (device) (syncthing_connections_active{${sel}}), 1)") "{{device}}")];
              mappings = [
                {
                  type = "value";
                  options = {
                    "0" = {
                      text = "Disconnected";
                      color = "red";
                      index = 0;
                    };
                    "1" = {
                      text = "Connected";
                      color = "green";
                      index = 1;
                    };
                  };
                }
              ];
            })
            (timeseries {
              title = "Sent per device";
              targets = [(pq (shortDevice "sum by (device) (rate(syncthing_protocol_sent_bytes_total{${sel}}[$__rate_interval]))") "{{device}}")];
              unit = "Bps";
            })
            (timeseries {
              title = "Received per device";
              targets = [(pq (shortDevice "sum by (device) (rate(syncthing_protocol_recv_bytes_total{${sel}}[$__rate_interval]))") "{{device}}")];
              unit = "Bps";
            })

            (row "Container" null)
            (timeseries {
              title = "Processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name="syncthing"}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name="syncthing"})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-syncthing.service|syncthing-config.service"} |~ "(?i)(error|warn|fail)"'')];
            })
          ];
        };
      };
    };
  };
}
