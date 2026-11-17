# Neo dashboard "Logs" (folder Neo). Not a module: imported by
# _grafana.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bars dashboard logs loki lq lqLegend pos queryVar text;
in
  dashboard {
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
  }
