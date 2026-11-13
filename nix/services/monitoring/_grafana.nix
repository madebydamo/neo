# Grafana dashboards and alert rules Neo provisions (not a module: imported by
# default.nix; import-tree skips `_` files).
#
# Metric and label names come from the agent (monitoring-agent/_alloy.nix):
#   node_* (job=node), container_* (job=containers, label name),
#   probe_* (job=uptime, label target), neo_service_unit{service,name,kind},
#   neo_auth_events_total{event,result,provider}, neo_http_requests_total{service,status_class},
#   neo_http_request_duration_seconds; every series carries host.
# Loki streams: job=journal {unit, level}, job=swag {service, status_class},
#   login events {event, result, provider} with username/ip as structured metadata.
{lib}: let
  prom = {
    type = "prometheus";
    uid = "prometheus";
  };
  loki = {
    type = "loki";
    uid = "loki";
  };

  pq = expr: legend: {
    datasource = prom;
    inherit expr;
    legendFormat = legend;
    range = true;
  };
  pqInstant = expr: legend: {
    datasource = prom;
    inherit expr;
    legendFormat = legend;
    instant = true;
    range = false;
    format = "table";
  };
  lq = expr: {
    datasource = loki;
    inherit expr;
    queryType = "range";
  };
  lqLegend = expr: legend: (lq expr) // {legendFormat = legend;};

  pos = x: y: w: h: {inherit x y w h;};

  thresholdsGreenRed = steps: {
    mode = "absolute";
    steps =
      [
        {
          color = "green";
          value = null;
        }
      ]
      ++ steps;
  };

  panel = {
    title,
    type,
    gridPos,
    targets,
    datasource ? prom,
    description ? "",
    unit ? null,
    min ? null,
    max ? null,
    thresholds ? null,
    mappings ? [],
    options ? {},
    custom ? {},
    overrides ? [],
    transformations ? [],
    decimals ? null,
  }: {
    inherit title type gridPos datasource description options transformations;
    targets = lib.imap0 (i: t: t // {refId = builtins.elemAt ["A" "B" "C" "D" "E" "F"] i;}) targets;
    fieldConfig = {
      defaults =
        {inherit mappings custom;}
        // lib.optionalAttrs (unit != null) {inherit unit;}
        // lib.optionalAttrs (min != null) {inherit min;}
        // lib.optionalAttrs (max != null) {inherit max;}
        // lib.optionalAttrs (decimals != null) {inherit decimals;}
        // lib.optionalAttrs (thresholds != null) {inherit thresholds;};
      inherit overrides;
    };
  };

  stat = args:
    panel ({
        type = "stat";
        options = {
          reduceOptions = {
            calcs = ["lastNotNull"];
            fields = "";
            values = false;
          };
          colorMode = "background";
          graphMode = "none";
          textMode = "value";
          justifyMode = "center";
        };
      }
      // args);

  timeseries = args:
    panel ({
        type = "timeseries";
        options = {
          legend = {
            displayMode = "list";
            placement = "bottom";
            showLegend = true;
          };
          tooltip = {
            mode = "multi";
            sort = "desc";
          };
        };
        custom = {
          drawStyle = "line";
          lineWidth = 1;
          fillOpacity = 10;
          showPoints = "never";
          spanNulls = true;
        };
      }
      // args);

  bars = args:
    timeseries (args
      // {
        custom = {
          drawStyle = "bars";
          fillOpacity = 80;
          lineWidth = 1;
          stacking = {
            mode = "normal";
            group = "A";
          };
        };
      });

  bargauge = args:
    panel ({
        type = "bargauge";
        options = {
          reduceOptions = {
            calcs = ["lastNotNull"];
            fields = "";
            values = false;
          };
          orientation = "horizontal";
          displayMode = "gradient";
          showUnfilled = true;
          minVizHeight = 10;
          minVizWidth = 0;
          namePlacement = "left";
        };
      }
      // args);

  logs = args:
    panel ({
        type = "logs";
        datasource = loki;
        options = {
          showTime = true;
          wrapLogMessage = true;
          prettifyLogMessage = false;
          enableLogDetails = true;
          sortOrder = "Descending";
          dedupStrategy = "none";
        };
      }
      // args);

  upDown = [
    {
      type = "value";
      options = {
        "0" = {
          text = "Down";
          color = "red";
          index = 0;
        };
        "1" = {
          text = "Up";
          color = "green";
          index = 1;
        };
      };
    }
  ];

  stateTimeline = args:
    panel ({
        type = "state-timeline";
        options = {
          showValue = "never";
          mergeValues = true;
          alignValue = "left";
          rowHeight = 0.8;
          legend = {
            showLegend = false;
            displayMode = "list";
            placement = "bottom";
          };
        };
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
        custom = {
          fillOpacity = 80;
          lineWidth = 0;
        };
      }
      // args);

  table = args:
    panel ({
        type = "table";
        options = {
          showHeader = true;
          cellHeight = "sm";
        };
      }
      // args);

  text = {
    title ? "",
    gridPos,
    content,
  }: {
    type = "text";
    inherit title gridPos;
    options = {
      mode = "markdown";
      inherit content;
    };
  };

  row = title: y: {
    type = "row";
    inherit title;
    collapsed = false;
    gridPos = pos 0 y 24 1;
    panels = [];
  };

  queryVar = {
    name,
    label,
    query,
    datasource ? prom,
    multi ? true,
    includeAll ? true,
  }: {
    inherit name label datasource multi includeAll;
    type = "query";
    query =
      if datasource == prom
      then {
        qryType = 1;
        inherit query;
        refId = "PrometheusVariableQueryEditor-VariableQuery";
      }
      else {
        type = 1;
        label = query.label;
        stream = query.stream or "";
        refId = "LokiVariableQueryEditor-VariableQuery";
      };
    definition =
      if datasource == prom
      then query
      else "label_values(${query.label})";
    refresh = 2;
    sort = 1;
    current = {};
    allValue = ".*";
  };

  hostVar = queryVar {
    name = "host";
    label = "Server";
    query = "label_values(node_boot_time_seconds, host)";
  };

  dashboard = {
    uid,
    title,
    panels,
    variables ? [],
    time ? "now-24h",
    refresh ? "1m",
    links ? [],
  }: {
    inherit uid title refresh;
    tags = ["neo"];
    timezone = "browser";
    editable = true;
    graphTooltip = 1;
    schemaVersion = 39;
    time = {
      from = time;
      to = "now";
    };
    templating.list = variables;
    links =
      [
        {
          title = "Neo dashboards";
          type = "dashboards";
          tags = ["neo"];
          asDropdown = true;
          includeVars = false;
          keepTime = true;
        }
      ]
      ++ links;
    # Panel ids in order.
    panels = lib.imap1 (i: p: p // {id = i;}) panels;
  };

  fsFilter = ''fstype!~"tmpfs|ramfs|devtmpfs|overlay|squashfs|nsfs|fuse.*|autofs|9p",mountpoint!~"/(nix/store|boot)"'';

  overview = dashboard {
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
        description = "Uptime checks that fail right now.";
        targets = [(pq "count(probe_success == 0) or vector(0)" "")];
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
        description = "Failed sign-ins (homeserver login and SSH) in the selected time range.";
        targets = [(pq ''round(sum(increase(neo_auth_events_total{event="login",result="failure"}[$__range]))) or vector(0)'' "")];
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
      (stateTimeline {
        title = "Uptime checks";
        gridPos = pos 0 12 12 10;
        description = "Each app's web address, checked every 30 seconds by the monitoring agents.";
        targets = [(pq "probe_success" "{{target}}")];
      })
      (stateTimeline {
        title = "Apps";
        gridPos = pos 12 12 12 10;
        description = "Up when every container of the app runs.";
        targets = [(pq ''min by (host, service) (neo_service_unit{kind="container"} * on (host, name) group_left () node_systemd_unit_state{state="active"})'' "{{service}} ({{host}})")];
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
        targets = [(pq "probe_duration_seconds" "{{target}}")];
        unit = "s";
      })

      (row "Logins" 30)
      (bars {
        title = "Sign-ins";
        gridPos = pos 0 31 12 8;
        description = "Homeserver login (tinyauth) and SSH sign-ins.";
        targets = [(pq ''sum by (result, provider) (increase(neo_auth_events_total{event="login"}[$__interval]))'' "{{provider}} {{result}}")];
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
  };

  host = dashboard {
    uid = "neo-server";
    title = "Server";
    time = "now-6h";
    variables = [(hostVar // {multi = false;})];
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
        gridPos = pos 12 4 12 8;
        targets = [
          (pq ''node_memory_MemTotal_bytes{host="$host"} - node_memory_MemAvailable_bytes{host="$host"}'' "used")
          (pq ''node_memory_MemTotal_bytes{host="$host"}'' "total")
        ];
        unit = "bytes";
        min = 0;
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
  };

  logsDash = dashboard {
    uid = "neo-logs";
    title = "Logs";
    time = "now-1h";
    refresh = "";
    variables = [
      (queryVar {
        name = "host";
        label = "Server";
        datasource = loki;
        query.label = "host";
      })
      (queryVar {
        name = "unit";
        label = "Unit";
        datasource = loki;
        query = {
          label = "unit";
          stream = ''{host=~"$host"}'';
        };
      })
      {
        name = "search";
        label = "Search";
        type = "textbox";
        query = "";
        current = {
          text = "";
          value = "";
        };
      }
    ];
    panels = [
      (bars {
        title = "Log lines";
        gridPos = pos 0 0 24 6;
        datasource = loki;
        targets = [(lqLegend ''sum by (unit) (count_over_time({job="journal", host=~"$host", unit=~"$unit"} |~ "(?i)$search" [$__auto]))'' "{{unit}}")];
        options.legend = {
          showLegend = false;
          displayMode = "list";
          placement = "bottom";
        };
      })
      (logs {
        title = "System log";
        gridPos = pos 0 6 24 16;
        targets = [(lq ''{job="journal", host=~"$host", unit=~"$unit"} |~ "(?i)$search"'')];
      })
      (logs {
        title = "Web requests";
        gridPos = pos 0 22 24 12;
        targets = [(lq ''{job="swag", host=~"$host"} |~ "(?i)$search"'')];
      })
    ];
  };

  security = dashboard {
    uid = "neo-logins";
    title = "Logins and access";
    time = "now-7d";
    panels = [
      (bars {
        title = "Sign-ins";
        gridPos = pos 0 0 24 7;
        targets = [(pq ''sum by (result, provider, host) (increase(neo_auth_events_total{event="login"}[$__interval]))'' "{{host}} {{provider}} {{result}}")];
      })
      (bargauge {
        title = "Failed sign-ins per user name";
        gridPos = pos 0 7 8 9;
        datasource = loki;
        targets = [((lqLegend ''topk(15, sum by (username) (count_over_time({event="login", result="failure"} [$__range])))'' "{{username}}") // {queryType = "instant";})];
        thresholds = thresholdsGreenRed [
          {
            color = "red";
            value = 5;
          }
        ];
      })
      (bargauge {
        title = "Failed sign-ins per address";
        gridPos = pos 8 7 8 9;
        datasource = loki;
        targets = [((lqLegend ''topk(15, sum by (ip) (count_over_time({event="login", result="failure"} [$__range])))'' "{{ip}}") // {queryType = "instant";})];
        thresholds = thresholdsGreenRed [
          {
            color = "red";
            value = 5;
          }
        ];
      })
      (bargauge {
        title = "Refused web requests per address";
        gridPos = pos 16 7 8 9;
        description = "Requests answered with 401, 403 or 404, by visitor address.";
        datasource = loki;
        targets = [((lqLegend ''topk(15, sum by (ip) (count_over_time({job="swag", status_class="4xx"} | status=~"401|403|404" [$__range])))'' "{{ip}}") // {queryType = "instant";})];
        thresholds = thresholdsGreenRed [
          {
            color = "orange";
            value = 100;
          }
        ];
      })
      (logs {
        title = "Sign-in events";
        gridPos = pos 0 16 24 12;
        targets = [(lq ''{event=~"login|logout"}'')];
      })
    ];
  };

  # Alert rules: query A (Prometheus, instant) → B (last value; summaries read
  # $values.B, raw queries are not in $values) → C (threshold on B). Each
  # query returns one series per thing that is wrong.
  rule = {
    folder ? "neo",
    group ? "neo",
    uid,
    title,
    expr,
    above ? 0,
    for ? "5m",
    summary,
    severity ? "warning",
  }: {
    inherit uid title for;
    condition = "C";
    data = [
      {
        refId = "A";
        relativeTimeRange = {
          from = 600;
          to = 0;
        };
        datasourceUid = "prometheus";
        model = {
          refId = "A";
          inherit expr;
          instant = true;
          range = false;
          intervalMs = 30000;
          maxDataPoints = 43200;
        };
      }
      {
        refId = "B";
        relativeTimeRange = {
          from = 0;
          to = 0;
        };
        datasourceUid = "__expr__";
        model = {
          refId = "B";
          type = "reduce";
          expression = "A";
          reducer = "last";
          settings.mode = "dropNN";
        };
      }
      {
        refId = "C";
        relativeTimeRange = {
          from = 0;
          to = 0;
        };
        datasourceUid = "__expr__";
        model = {
          refId = "C";
          type = "threshold";
          expression = "B";
          conditions = [
            {
              evaluator = {
                type = "gt";
                params = [above];
              };
            }
          ];
        };
      }
    ];
    folderUID = folder;
    ruleGroup = group;
    orgID = 1;
    noDataState = "OK";
    execErrState = "Error";
    labels.severity = severity;
    annotations.summary = summary;
    isPaused = false;
  };

  alertRules =
    [
      (rule {
        uid = "neo-app-down";
        title = "App does not answer";
        expr = "1 - probe_success";
        for = "5m";
        severity = "critical";
        summary = "{{ $labels.target }} has not answered for 5 minutes (checked from {{ $labels.host }}).";
      })
      (rule {
        uid = "neo-server-silent";
        title = "Server stopped reporting";
        expr = "group by (host) (max_over_time(node_boot_time_seconds[1d])) unless group by (host) (node_boot_time_seconds)";
        for = "5m";
        severity = "critical";
        summary = "{{ $labels.host }} has sent no numbers for over 5 minutes: it is off, offline or its monitoring agent stopped.";
      })
      (rule {
        uid = "neo-cert-expiry";
        title = "Certificate expires soon";
        # Days left below 10 (SWAG renews 30 days ahead).
        expr = "10 - (min by (app, target) (probe_ssl_earliest_cert_expiry) - time()) / 86400";
        for = "1h";
        summary = "The HTTPS certificate of {{ $labels.target }} expires in less than 10 days and was not renewed.";
      })
      (rule {
        uid = "neo-disk-full";
        title = "Disk almost full";
        expr = "100 * (1 - node_filesystem_avail_bytes{${fsFilter}} / node_filesystem_size_bytes{${fsFilter}})";
        above = 90;
        for = "15m";
        summary = "{{ $labels.mountpoint }} on {{ $labels.host }} is {{ printf \"%.0f\" $values.B.Value }}% full.";
      })
      (rule {
        uid = "neo-unit-failed";
        title = "Service failed";
        expr = ''node_systemd_unit_state{state="failed"} == 1'';
        for = "10m";
        summary = "{{ $labels.name }} on {{ $labels.host }} has failed.";
      })
      (rule {
        uid = "neo-failed-logins";
        title = "Many failed sign-ins";
        expr = ''sum by (host, provider) (increase(neo_auth_events_total{event="login",result="failure"}[15m]))'';
        above = 10;
        for = "0s";
        summary = "{{ printf \"%.0f\" $values.B.Value }} failed {{ $labels.provider }} sign-ins on {{ $labels.host }} in the last 15 minutes.";
      })
      (rule {
        uid = "neo-server-errors";
        title = "App answers with errors";
        expr = ''(sum by (host, service) (rate(neo_http_requests_total{status_class="5xx"}[10m])) / sum by (host, service) (rate(neo_http_requests_total[10m]))) and on (host, service) sum by (host, service) (rate(neo_http_requests_total{status_class="5xx"}[10m])) > 0.02'';
        above = 0.2;
        for = "10m";
        summary = "{{ humanizePercentage $values.B.Value }} of requests to {{ $labels.service }} on {{ $labels.host }} end in a server error.";
      })
    ]
    ++ iso.alertRules;

  helpers = {
    inherit prom loki pq pqInstant lq lqLegend pos thresholdsGreenRed panel stat timeseries bars bargauge logs stateTimeline table row queryVar hostVar dashboard fsFilter rule text;
  };
  iso = import ./_iso27001.nix {
    inherit lib;
    h = helpers;
  };
in {
  # Grafana folders (uid → title) and dashboards (name → folder + model).
  folders =
    {
      neo.title = "Neo";
    }
    // iso.folders;
  dashboards =
    lib.mapAttrs (_: dashboard: {
      folder = "neo";
      inherit dashboard;
    }) {
      overview = overview;
      server = host;
      logs = logsDash;
      logins = security;
    }
    // iso.dashboards;
  inherit alertRules;
}
