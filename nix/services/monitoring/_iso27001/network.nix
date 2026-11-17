# ISO 27001 dashboard (folder "ISO 27001"). Not a module: imported by
# _iso27001.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge bars instantLoki iso logs loki lq orange pos pq red text thresholdsGreenRed timeseries;
in
  iso {
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
      (g.panel {
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
  }
