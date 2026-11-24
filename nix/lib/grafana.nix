# Grafana dashboard helpers: lib.neo.grafana.
#
# Build Grafana dashboard JSON models in Nix. Used by Neo's own dashboards
# (nix/services/monitoring) and by app dashboards any service or plugin declares
# in neo.monitoring.apps.<service>.dashboards (nix/modules/core/monitoring.nix).
#
#   with lib.neo.grafana; [
#     (timeseries { title = "Requests"; targets = [(pq "sum(rate(x_total{${appSelector "immich"}}[5m]))" "requests")]; })
#     (stat { title = "Users"; w = 6; h = 4; targets = [(pq "max(x_users{${appSelector "immich"}})" "")]; })
#   ]
#
# Panels without gridPos are laid out automatically (w/h in 24-column grid units).
# Datasources: `prom` (uid prometheus) and `loki` (uid loki), provisioned by monitoring.
{lib, ...}: {
  libExtensions.grafana.neo.grafana = rec {
    prom = {
      type = "prometheus";
      uid = "prometheus";
    };
    loki = {
      type = "loki";
      uid = "loki";
    };

    # Query letter for the i-th target (0-based): A … Z, then AA, AB, … like Grafana.
    refId = i: let
      letters = lib.stringToCharacters "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    in
      if i < 26
      then builtins.elemAt letters i
      else refId (i / 26 - 1) + builtins.elemAt letters (lib.mod i 26);

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
      gridPos ? null,
      # Size for the automatic layout when gridPos is null (24 columns wide).
      w ? 12,
      h ? 8,
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
      # Shortest step Grafana uses for $__interval. The Prometheus datasource
      # says 30s (the scrape interval), so increase(x[$__interval]) would see a
      # single sample and draw nothing; 1m always holds two.
      interval ? "1m",
      # Text shown instead of "No data".
      noValue ? null,
    }: {
      inherit title type datasource description options transformations interval;
      gridPos =
        if gridPos != null
        then gridPos
        else {
          inherit w h;
          x = null;
          y = null;
        };
      targets = lib.imap0 (i: t: t // {refId = refId i;}) targets;
      fieldConfig = {
        defaults =
          {inherit mappings custom;}
          // lib.optionalAttrs (unit != null) {inherit unit;}
          // lib.optionalAttrs (min != null) {inherit min;}
          // lib.optionalAttrs (max != null) {inherit max;}
          // lib.optionalAttrs (decimals != null) {inherit decimals;}
          // lib.optionalAttrs (noValue != null) {inherit noValue;}
          // lib.optionalAttrs (thresholds != null) {inherit thresholds;};
        inherit overrides;
      };
    };

    # `options` (and `custom`) given by the caller are merged into the defaults,
    # so `options.colorMode = "none"` keeps reduceOptions.
    withDefaults = defaults: args:
      defaults
      // args
      // lib.optionalAttrs (args ? options) {options = (defaults.options or {}) // args.options;}
      // lib.optionalAttrs (args ? custom && defaults ? custom) {custom = defaults.custom // args.custom;};

    stat = args:
      panel (withDefaults {
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
        args);

    timeseries = args:
      panel (withDefaults {
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
        args);

    bars = args:
      timeseries ({interval = "5m";}
        // args
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

    # A Loki bar gauge (e.g. topk(…) by user name) runs as a range query whose
    # last value per series becomes one sorted row: Grafana draws a Loki
    # instant result as a single nameless bar.
    bargauge = args: let
      isLoki = (args.datasource or prom) == loki;
    in
      panel ({
          type = "bargauge";
          options = {
            reduceOptions = {
              calcs = ["lastNotNull"];
              fields = "";
              values = isLoki;
            };
            orientation = "horizontal";
            displayMode = "gradient";
            showUnfilled = true;
            minVizHeight = 10;
            minVizWidth = 0;
            namePlacement = "left";
          };
        }
        // args
        # Prometheus: one instant value per series is all a bar gauge shows,
        # and sort_desc/topk only order an instant result.
        // lib.optionalAttrs (!isLoki) {
          targets = map (t:
            t
            // {
              instant = true;
              range = false;
            })
          args.targets;
        }
        // lib.optionalAttrs isLoki {
          targets = map (t: t // {queryType = "range";}) args.targets;
          transformations =
            [
              {
                id = "reduce";
                options = {
                  reducers = ["lastNotNull"];
                  mode = "seriesToRows";
                };
              }
              {
                id = "sortBy";
                options.sort = [
                  {
                    field = "Last *";
                    desc = true;
                  }
                ];
              }
            ]
            ++ (args.transformations or []);
        });

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
      gridPos ? null,
      w ? 24,
      h ? 3,
      content,
    }: {
      type = "text";
      inherit title;
      gridPos =
        if gridPos != null
        then gridPos
        else {
          inherit w h;
          x = null;
          y = null;
        };
      options = {
        mode = "markdown";
        inherit content;
      };
    };

    # A section header. `y` may be null inside an automatic layout.
    row = title: y: {
      type = "row";
      inherit title;
      collapsed = false;
      gridPos = {
        x =
          if y == null
          then null
          else 0;
        inherit y;
        w = 24;
        h = 1;
      };
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

    # Panels whose gridPos has x = null flow left to right, wrapping at 24
    # columns; a row header or a fixed gridPos starts a new line below.
    layout = panels:
      (lib.foldl' (st: p: let
          g = p.gridPos;
          auto = g.x == null;
          newLine = st.x + g.w > 24 || p.type == "row";
          y0 =
            if auto && newLine
            then st.y + st.rowH
            else st.y;
          x0 =
            if auto && newLine
            then 0
            else st.x;
          placed =
            if auto
            then
              p
              // {
                gridPos =
                  g
                  // {
                    x = x0;
                    y = y0;
                  };
              }
            else p;
          pg = placed.gridPos;
        in
          if auto
          then {
            acc = st.acc ++ [placed];
            x = x0 + g.w;
            y = y0;
            rowH =
              if newLine
              then g.h
              else lib.max st.rowH g.h;
          }
          else {
            acc = st.acc ++ [placed];
            x = 24;
            y = lib.max st.y pg.y;
            rowH = lib.max 0 (pg.y + pg.h - lib.max st.y pg.y);
          }) {
          acc = [];
          x = 0;
          y = 0;
          rowH = 0;
        }
        panels)
        .acc;

    # A label selector for app dashboards: the service's series on the chosen servers.
    appSelector = service: ''service="${service}", host=~"$host"'';

    # A selector for app alert rules: no $host variable there (rules see every
    # server), so only the service.
    alertSelector = service: ''service="${service}"'';

    # One Grafana alert rule (provisioning format, without folder and group).
    # Query A (instant; Prometheus or a Loki metric query) → B (last value;
    # summaries read $values.B) → C (B above `above`, or below `below`). Each
    # series A returns is one alert instance, so return one series per thing
    # that is wrong and keep its labels (host, …) for the summary.
    # `recover`: a firing instance goes back to normal only when B is below it
    # (an `above` rule) or above it (a `below` rule), Grafana's recovery
    # threshold, so a value around the threshold does not flap Normal/Alerting
    # every minute (an increase() count of 10 real events reads 9.3 or 10.3).
    alertRule = {
      uid,
      title,
      expr,
      summary,
      datasource ? "prometheus",
      above ? null,
      below ? null,
      for ? "5m",
      severity ? "warning",
      noData ? "ok",
      labels ? {},
      range ? 600,
      recover ? null,
    }: let
      lt = below != null;
      threshold =
        if lt
        then below
        else if above == null
        then 0
        else above;
      # recover must lie on the normal side of the threshold.
      recoverOk =
        if lt
        then recover >= threshold
        else recover <= threshold;
    in {
      inherit uid title for;
      condition = "C";
      data = [
        {
          refId = "A";
          relativeTimeRange = {
            from = range;
            to = 0;
          };
          datasourceUid = datasource;
          model =
            {
              refId = "A";
              inherit expr;
              intervalMs = 30000;
              maxDataPoints = 43200;
            }
            // (
              if datasource == "loki"
              then {queryType = "instant";}
              else {
                instant = true;
                range = false;
              }
            );
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
              ({
                  evaluator = {
                    type =
                      if lt
                      then "lt"
                      else "gt";
                    params = [threshold];
                  };
                }
                // lib.optionalAttrs (recover != null) {
                  # Grafana inverts it: the instance recovers while this is true.
                  unloadEvaluator = {
                    type =
                      if lt
                      then "gt"
                      else "lt";
                    params = [(lib.throwIfNot recoverOk "alert rule ${uid}: recover (${toString recover}) must be on the normal side of the threshold (${toString threshold})" recover)];
                  };
                })
            ];
          };
        }
      ];
      noDataState =
        {
          ok = "OK";
          alerting = "Alerting";
          nodata = "NoData";
        }
        .${
          noData
        };
      execErrState = "Error";
      labels = {inherit severity;} // labels;
      annotations.summary = summary;
      isPaused = false;
    };

    dashboard = {
      uid,
      title,
      panels,
      variables ? [],
      time ? "now-24h",
      refresh ? "1m",
      links ? [],
      tags ? ["neo"],
      description ? "",
    }: {
      inherit uid title refresh tags description;
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
      panels = lib.imap1 (i: p: p // {id = i;}) (layout panels);
    };
  };
}
