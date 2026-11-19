# Neo dashboard "Server" (folder Neo). Not a module: imported by
# _grafana.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge dashboard fsFilter hostVar logs lq pos pq pqInstant stat table thresholdsGreenRed timeseries;

  # Memory breakdown. sum() drops the labels so that the ARC term can fall
  # back to vector(0) on a host without ZFS. The ARC is not in Cached and not
  # in MemAvailable, so "used" alone counted it as app memory.
  mem = field: ''sum(node_memory_${field}_bytes{host="$host"})'';
  arc = ''(sum(node_zfs_arc_size{host="$host"}) or vector(0))'';
  # Page cache plus reclaimable kernel caches; Shmem (tmpfs, shared memory)
  # sits in Cached but cannot be dropped, so it counts as apps.
  memCache = "(${mem "Buffers"} + ${mem "Cached"} + ${mem "SReclaimable"} - ${mem "Shmem"})";
  memColor = name: color: {
    matcher = {
      id = "byName";
      options = name;
    };
    properties = [
      {
        id = "color";
        value = {
          mode = "fixed";
          fixedColor = color;
        };
      }
    ];
  };
in
  dashboard {
    uid = "neo-server";
    title = "Server";
    time = "now-6h";
    # One server at a time: the queries compare host="$host" (no "All").
    variables = [
      (hostVar
        // {
          multi = false;
          includeAll = false;
        })
    ];
    panels = [
      (stat {
        title = "Running since";
        gridPos = pos 0 0 4 4;
        targets = [(pq ''node_time_seconds{host="$host"} - node_boot_time_seconds{host="$host"}'' "")];
        unit = "s";
        options.colorMode = "value";
        thresholds = thresholdsGreenRed [];
      })
      (stat {
        title = "Processors";
        gridPos = pos 4 0 4 4;
        targets = [(pq ''count(node_cpu_seconds_total{host="$host",mode="idle"})'' "")];
        options.colorMode = "value";
        thresholds = thresholdsGreenRed [];
      })
      (stat {
        title = "Memory";
        gridPos = pos 8 0 4 4;
        targets = [(pq ''node_memory_MemTotal_bytes{host="$host"}'' "")];
        unit = "bytes";
        options.colorMode = "value";
        thresholds = thresholdsGreenRed [];
      })
      (stat {
        title = "Failed units";
        gridPos = pos 12 0 4 4;
        targets = [(pq ''sum(node_systemd_unit_state{host="$host",state="failed"}) or vector(0)'' "")];
        thresholds = thresholdsGreenRed [
          {
            color = "red";
            value = 1;
          }
        ];
      })
      (table {
        title = "Failed units";
        gridPos = pos 16 0 8 4;
        targets = [(pqInstant ''node_systemd_unit_state{host="$host",state="failed"} == 1'' "")];
        transformations = [
          {
            id = "organize";
            options = {
              excludeByName = {
                Time = true;
                Value = true;
                __name__ = true;
                instance = true;
                job = true;
                host = true;
                state = true;
                type = true;
              };
              renameByName.name = "Unit";
            };
          }
        ];
      })

      (timeseries {
        title = "Processor";
        gridPos = pos 0 4 12 8;
        targets = [(pq ''100 * sum by (mode) (rate(node_cpu_seconds_total{host="$host",mode!="idle"}[$__rate_interval])) / scalar(count(node_cpu_seconds_total{host="$host",mode="idle"}))'' "{{mode}}")];
        unit = "percent";
        min = 0;
        custom = {
          drawStyle = "line";
          fillOpacity = 30;
          lineWidth = 1;
          showPoints = "never";
          stacking = {
            mode = "normal";
            group = "A";
          };
        };
      })
      (timeseries {
        title = "Memory";
        description = "Apps: what programs and the system hold. ZFS cache: recently read disk data, given back when apps need memory. File cache: the same for other files. Free: not used at all.";
        gridPos = pos 12 4 12 8;
        targets = [
          (pq "clamp_min(${mem "MemTotal"} - ${mem "MemFree"} - ${memCache} - ${arc}, 0)" "Apps")
          (pq arc "ZFS cache (ARC)")
          (pq memCache "File cache")
          (pq (mem "MemFree") "Free")
          (pq (mem "MemTotal") "Total")
        ];
        unit = "bytes";
        min = 0;
        custom = {
          fillOpacity = 60;
          stacking = {
            mode = "normal";
            group = "A";
          };
        };
        overrides = [
          (memColor "Apps" "orange")
          (memColor "ZFS cache (ARC)" "purple")
          (memColor "File cache" "blue")
          (memColor "Free" "green")
          {
            matcher = {
              id = "byName";
              options = "Total";
            };
            properties = [
              {
                id = "custom.stacking";
                value.mode = "none";
              }
              {
                id = "custom.fillOpacity";
                value = 0;
              }
              {
                id = "custom.lineStyle";
                value = {
                  fill = "dash";
                  dash = [10 10];
                };
              }
              {
                id = "color";
                value = {
                  mode = "fixed";
                  fixedColor = "text";
                };
              }
            ];
          }
        ];
      })
      (bargauge {
        title = "Disks";
        gridPos = pos 0 12 12 8;
        targets = [(pq ''100 * (1 - node_filesystem_avail_bytes{host="$host",${fsFilter}} / node_filesystem_size_bytes{host="$host",${fsFilter}})'' "{{mountpoint}}")];
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
      (timeseries {
        title = "Disk activity";
        gridPos = pos 12 12 12 8;
        targets = [
          (pq ''sum by (device) (rate(node_disk_read_bytes_total{host="$host"}[$__rate_interval]))'' "{{device}} read")
          (pq ''-sum by (device) (rate(node_disk_written_bytes_total{host="$host"}[$__rate_interval]))'' "{{device}} write")
        ];
        unit = "Bps";
      })
      (timeseries {
        title = "Network";
        gridPos = pos 0 20 12 8;
        targets = [
          (pq ''sum by (device) (rate(node_network_receive_bytes_total{host="$host",device!~"lo|veth.*|br-.*|docker.*"}[$__rate_interval]))'' "{{device}} in")
          (pq ''-sum by (device) (rate(node_network_transmit_bytes_total{host="$host",device!~"lo|veth.*|br-.*|docker.*"}[$__rate_interval]))'' "{{device}} out")
        ];
        unit = "Bps";
      })
      (timeseries {
        title = "Load";
        gridPos = pos 12 20 12 8;
        targets = [
          (pq ''node_load1{host="$host"}'' "1 min")
          (pq ''node_load5{host="$host"}'' "5 min")
          (pq ''node_load15{host="$host"}'' "15 min")
        ];
      })
      (timeseries {
        title = "Containers: processor";
        gridPos = pos 0 28 12 9;
        targets = [(pq ''topk(10, sum by (name) (rate(container_cpu_usage_seconds_total{host="$host",name!=""}[$__rate_interval])))'' "{{name}}")];
        unit = "percentunit";
      })
      (timeseries {
        title = "Containers: memory";
        gridPos = pos 12 28 12 9;
        targets = [(pq ''topk(10, sum by (name) (container_memory_working_set_bytes{host="$host",name!=""}))'' "{{name}}")];
        unit = "bytes";
      })
      (logs {
        title = "Errors in the system log";
        gridPos = pos 0 37 24 10;
        targets = [(lq ''{job="journal", host="$host", level=~"emerg|alert|crit|err"}'')];
      })
    ];
  }
