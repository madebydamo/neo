# SearXNG metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# SearXNG serves /metrics only while `general.open_metrics` in settings.yml
# holds a password (basic auth, the username is ignored). SearXNG has no
# environment variable for it, and settings.yml belongs to the operator (the
# image writes it from its template on first start, Neo does not render it).
# So the setup unit searxng-metrics edits that one key with yq: the app's
# scrapeToken while a monitoring agent runs here, empty otherwise. The token
# comes from a rendered secrets file at run time, never from the store. When
# the value changed it restarts searxng once (--no-block: the restart also
# restarts this unit through Requires=, which then finds nothing to do).
#
# Metrics (searx/metrics/__init__.py openmetrics(), label engine_name). Only
# searxng_engines_request_count_total is a real counter; the others are
# medians or percentages over the process lifetime despite their names, and
# engines with a value of 0 are left out:
#   searxng_engines_request_count_total            searches sent to the engine
#   searxng_engines_result_count_total             median results per search
#   searxng_engines_reliability_total              reliability in percent (100 - errors)
#   searxng_engines_response_time_{total,http,processing}_seconds   median times
#
# With vpn.enabled searxng lives in gluetun's network namespace: the agent
# scrapes the `vpn` container on searxng's network, where gluetun answers.
{...}: {
  flake.modules.nixos.searxng-monitoring = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.searxng;
    app = config.neo.monitoring.apps.searxng;
    collecting = config.neo.monitoring.collecting;
    sel = lib.neo.grafana.appSelector "searxng";
    settingsFile = "${cfg.appdata}/searxng/settings.yml";
    tokenTemplate = "searxng-open-metrics";

    # Engines that answered at all (the value is left out at 0).
    eng = metric: ''${metric}{${sel}}'';
  in {
    config = lib.mkMerge [
      # Same priority as the option's default, so it adds to the units from
      # mkContainerDefinitions instead of replacing them. Not under mkIf: a
      # definition of neo.services.searxng must not depend on cfg.enabled.
      {neo.services.searxng.systemdUnits = lib.mkOptionDefault ["searxng-metrics"];}
      (lib.mkIf cfg.enabled {
        neo.core.secrets.templates.${tokenTemplate} = lib.mkIf collecting {
          content = app.scrapeToken;
          restartUnits = ["searxng-metrics.service"];
        };

        systemd.services.searxng-metrics = lib.neo.mkSetupService {
          inherit pkgs;
          name = "searxng-metrics";
          description = "Set the SearXNG metrics password for the monitoring agent";
          containers = ["searxng"];
          path = [pkgs.yq-go pkgs.coreutils pkgs.systemd];
          retryInterval = 10;
          script = ''
            SETTINGS="${settingsFile}"
            if [ ! -f "$SETTINGS" ]; then
              echo "searxng settings.yml not written yet"
              exit 1
            fi
            ${
              if collecting
              then ''want="$(cat ${config.neo.core.secrets.templates.${tokenTemplate}.path})"''
              else "want=''"
            }
            have="$(yq -r '.general.open_metrics // ""' "$SETTINGS")"
            if [ "$have" = "$want" ]; then
              echo "open_metrics up to date"
              exit 0
            fi
            tmp="$(mktemp)"
            trap 'rm -f "$tmp"' EXIT
            OPEN_METRICS="$want" yq '.general.open_metrics = strenv(OPEN_METRICS)' "$SETTINGS" > "$tmp"
            # Write through the existing file: keeps its owner and mode.
            cat "$tmp" > "$SETTINGS"
            echo "open_metrics changed, restarting searxng"
            systemctl restart --no-block docker-searxng.service
          '';
        };

        neo.monitoring.apps.searxng = {
          hidePaths = ["/metrics"];

          metrics.main =
            {
              port = 8080;
              auth = "basic";
            }
            // (
              if cfg.vpn.enabled
              then {
                container = "vpn";
                network = lib.neo.docker.serviceNetwork "searxng";
              }
              else {container = "searxng";}
            );

          dashboards.overview = {
            title = "SearXNG";
            description = "Searches, and how the search engines SearXNG asks behave: how often, how fast, how reliable and how many results.";
            panels = with lib.neo.grafana; [
              (row "Searches" null)
              (stat {
                title = "Running";
                w = 4;
                h = 4;
                description = "SearXNG answers the monitoring agent.";
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
                title = "Page views";
                w = 5;
                h = 4;
                description = "Requests to the search site in the selected time range (searches, result pages, images and other files).";
                targets = [(pq ''round(sum(increase(neo_http_requests_total{host=~"$host", service="searxng"}[$__range]))) or vector(0)'' "")];
                options.colorMode = "value";
                thresholds = thresholdsGreenRed [];
              })
              (stat {
                title = "Server errors";
                w = 5;
                h = 4;
                description = "Requests that failed on the server side (5xx) in the selected time range.";
                targets = [(pq ''round(sum(increase(neo_http_requests_total{host=~"$host", service="searxng", status_class="5xx"}[$__range]))) or vector(0)'' "")];
                thresholds = thresholdsGreenRed [
                  {
                    color = "red";
                    value = 1;
                  }
                ];
              })
              (stat {
                title = "Engines in use";
                w = 5;
                h = 4;
                description = "Search engines that got at least one search since SearXNG started.";
                targets = [(pq "count(${eng "searxng_engines_request_count_total"} > 0) or vector(0)" "")];
                options.colorMode = "value";
                thresholds = thresholdsGreenRed [];
              })
              (stat {
                title = "Lowest reliability";
                w = 5;
                h = 4;
                description = "The engine that fails most often: share of its searches that worked.";
                targets = [(pq "min(${eng "searxng_engines_reliability_total"})" "")];
                unit = "percent";
                decimals = 0;
                thresholds = {
                  mode = "absolute";
                  steps = [
                    {
                      color = "red";
                      value = null;
                    }
                    {
                      color = "orange";
                      value = 50;
                    }
                    {
                      color = "green";
                      value = 90;
                    }
                  ];
                };
              })
              (bars {
                title = "Requests to the search site";
                w = 24;
                h = 7;
                description = "From the web server in front of SearXNG, by result: 2xx worked, 3xx redirect, 4xx refused or not found, 5xx server error.";
                targets = [(pq ''sum by (status_class) (increase(neo_http_requests_total{host=~"$host", service="searxng"}[$__interval]))'' "{{status_class}}")];
              })

              (row "Search engines" null)
              (timeseries {
                title = "Searches per engine";
                description = "How often SearXNG asks each search engine.";
                targets = [(pq "sum by (engine_name) (rate(${eng "searxng_engines_request_count_total"}[$__rate_interval]))" "{{engine_name}}")];
                unit = "reqps";
              })
              (bargauge {
                title = "Searches per engine in this time range";
                targets = [(pq "sort_desc(sum by (engine_name) (increase(${eng "searxng_engines_request_count_total"}[$__range])) > 0)" "{{engine_name}}")];
                decimals = 0;
                thresholds = thresholdsGreenRed [];
              })
              (bargauge {
                title = "Reliability per engine";
                description = "Share of searches that worked, since SearXNG started. Low values mean the engine blocks or times out; routing through a VPN or turning the engine off can help.";
                targets = [(pq "sort(max by (engine_name) (${eng "searxng_engines_reliability_total"}))" "{{engine_name}}")];
                unit = "percent";
                min = 0;
                max = 100;
                decimals = 0;
                thresholds = {
                  mode = "absolute";
                  steps = [
                    {
                      color = "red";
                      value = null;
                    }
                    {
                      color = "orange";
                      value = 50;
                    }
                    {
                      color = "green";
                      value = 90;
                    }
                  ];
                };
              })
              (bargauge {
                title = "Results per search";
                description = "Typical number of results each engine returns for one search (median since SearXNG started).";
                targets = [(pq "sort_desc(max by (engine_name) (${eng "searxng_engines_result_count_total"}))" "{{engine_name}}")];
                decimals = 0;
                thresholds = thresholdsGreenRed [];
              })
              (timeseries {
                title = "Response time per engine";
                description = "Typical time an engine takes to answer, including SearXNG's own work on the results (median since SearXNG started).";
                targets = [(pq "max by (engine_name) (${eng "searxng_engines_response_time_total_seconds"})" "{{engine_name}}")];
                unit = "s";
              })
              (bargauge {
                title = "Slowest engines";
                description = "Median answer time: waiting for the engine's website, and the total including SearXNG's processing.";
                targets = [
                  (pq "topk(10, max by (engine_name) (${eng "searxng_engines_response_time_total_seconds"}))" "{{engine_name}} total")
                  (pq "topk(10, max by (engine_name) (${eng "searxng_engines_response_time_http_seconds"}))" "{{engine_name}} website")
                ];
                unit = "s";
                thresholds = thresholdsGreenRed [
                  {
                    color = "orange";
                    value = 2;
                  }
                  {
                    color = "red";
                    value = 5;
                  }
                ];
              })

              (row "Containers" null)
              (timeseries {
                title = "Processor";
                targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name=~"searxng.*"}[$__rate_interval]))'' "{{name}}")];
                unit = "percentunit";
              })
              (timeseries {
                title = "Memory";
                targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name=~"searxng.*"})'' "{{name}}")];
                unit = "bytes";
              })
              (logs {
                title = "Errors and warnings";
                w = 24;
                h = 10;
                targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-searxng.*|searxng-metrics.*"} |~ "(?i)(error|warn|exception|fail)"'')];
              })
            ];
          };
        };
      })
    ];
  };
}
