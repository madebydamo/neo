# Neo dashboard "Logins and access" (folder Neo). Not a module: imported by
# _grafana.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge bars dashboard logs loki lq lqLegend pos pq thresholdsGreenRed;
in
  dashboard {
    uid = "neo-logins";
    title = "Logins and access";
    time = "now-7d";
    panels = [
      (bars {
        title = "Sign-ins";
        gridPos = pos 0 0 24 7;
        description = "Sign-ins by server, method and result. scan: SSH attempts with a user name that does not exist on a server that accepts keys only (Internet scanners); they cannot succeed and raise no alert.";
        # attempt="scan" (monitoring-agent/_alloy.nix) shown as its own result.
        targets = [(pq ''sum by (result, provider, host) (label_replace(increase(neo_auth_events_total{event="login"}[$__interval]), "result", "scan", "attempt", "scan"))'' "{{host}} {{provider}} {{result}}")];
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
  }
