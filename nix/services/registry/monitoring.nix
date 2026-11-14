# Docker registry metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# Distribution serves Prometheus metrics on its debug listener, which the stock
# config.yml of the image does not turn on: while a monitoring agent runs on
# this machine the env pins it to :5001/metrics. That port also serves pprof;
# it stays on the registry's own Docker network and is never published or proxied.
#
# Metric names (distribution v3, docker/go-metrics):
#   registry_http_requests_total{handler, code, method}        handler = route name
#   registry_http_request_duration_seconds_*{handler, method}  (manifest, blob, blob_upload, …)
#   registry_storage_action_seconds_*{driver, action}           storage driver calls
#   registry_storage_cache_{requests,hits,errors}_total         blob descriptor cache
# promhttp lowercases method ("get", "put"), so the queries match it case-insensitively.
{...}: {
  flake.modules.nixos.registry-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.registry;
    sel = lib.neo.grafana.appSelector "registry";
    debugPort = 5001;

    manifest = method: ''${sel}, handler="manifest", method=~"(?i)${method}", code=~"2.."'';
    pushes = manifest "put";
    pulls = manifest "get";
  in {
    config = lib.mkIf cfg.enabled {
      virtualisation.oci-containers.containers.registry.environment = lib.mkIf config.neo.monitoring.collecting {
        REGISTRY_HTTP_DEBUG_ADDR = "0.0.0.0:${toString debugPort}";
        REGISTRY_HTTP_DEBUG_PROMETHEUS_ENABLED = "true";
        REGISTRY_HTTP_DEBUG_PROMETHEUS_PATH = "/metrics";
      };

      neo.monitoring.apps.registry = {
        metrics.registry = {
          container = "registry";
          port = debugPort;
        };

        dashboards.overview = {
          title = "Registry";
          description = "Image uploads and downloads, response times, errors and storage of the Docker registry.";
          time = "now-7d";
          panels = with lib.neo.grafana; [
            (row "Overview" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "The registry answers.";
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
              title = "Images uploaded";
              w = 4;
              h = 4;
              description = "Image tags pushed in the selected time range.";
              targets = [(pq "round(sum(increase(registry_http_requests_total{${pushes}}[$__range]))) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Images downloaded";
              w = 4;
              h = 4;
              description = "Image pulls in the selected time range.";
              targets = [(pq "round(sum(increase(registry_http_requests_total{${pulls}}[$__range]))) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Server errors";
              w = 4;
              h = 4;
              description = "Requests the registry could not answer because of a problem on its side, in the selected time range.";
              targets = [(pq ''round(sum(increase(registry_http_requests_total{${sel}, code=~"5.."}[$__range]))) or vector(0)'' "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "red";
                  value = 1;
                }
              ];
            })
            (stat {
              title = "Cache hits";
              w = 4;
              h = 4;
              description = "Share of image lookups answered from memory instead of the disk, in the selected time range.";
              targets = [(pq "sum(increase(registry_storage_cache_hits_total{${sel}}[$__range])) / sum(increase(registry_storage_cache_requests_total{${sel}}[$__range]) > 0)" "")];
              unit = "percentunit";
              decimals = 0;
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Memory";
              w = 4;
              h = 4;
              targets = [(pq ''sum(container_memory_working_set_bytes{host=~"$host", name="registry"})'' "")];
              unit = "bytes";
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (bars {
              title = "Uploads and downloads";
              w = 24;
              h = 7;
              description = "Image tags pushed and pulled.";
              targets = [
                (pq "sum(increase(registry_http_requests_total{${pushes}}[$__interval]))" "uploaded")
                (pq "sum(increase(registry_http_requests_total{${pulls}}[$__interval]))" "downloaded")
              ];
            })

            (row "Requests" null)
            (timeseries {
              title = "Requests by kind";
              description = "Requests per second: image descriptions (manifest), layers (blob), uploads, tags and the catalog.";
              targets = [(pq "sum by (handler) (rate(registry_http_requests_total{${sel}}[$__rate_interval]))" "{{handler}}")];
              unit = "reqps";
            })
            (timeseries {
              title = "Response time (95%)";
              description = "How long 95% of the requests of each kind took. Layer uploads and downloads take as long as the transfer.";
              targets = [(pq "histogram_quantile(0.95, sum by (handler, le) (rate(registry_http_request_duration_seconds_bucket{${sel}}[$__rate_interval])))" "{{handler}}")];
              unit = "s";
            })
            (timeseries {
              title = "Server errors";
              description = "Requests per second that failed on the registry's side.";
              targets = [(pq ''sum by (handler, code) (rate(registry_http_requests_total{${sel}, code=~"5.."}[$__rate_interval]))'' "{{handler}} {{code}}")];
              unit = "reqps";
            })
            (timeseries {
              title = "Refused requests";
              description = "Requests per second the registry turned down: wrong login (401), not allowed (403) or not found (404).";
              targets = [(pq ''sum by (handler, code) (rate(registry_http_requests_total{${sel}, code=~"4.."}[$__rate_interval]))'' "{{handler}} {{code}}")];
              unit = "reqps";
            })

            (row "Storage" null)
            (timeseries {
              title = "Storage time (95%)";
              description = "How long reading and writing image data on disk took, per kind of action.";
              targets = [(pq "histogram_quantile(0.95, sum by (action, le) (rate(registry_storage_action_seconds_bucket{${sel}}[$__rate_interval])))" "{{action}}")];
              unit = "s";
            })
            (timeseries {
              title = "Storage actions";
              description = "Disk actions per second, per kind of action.";
              targets = [(pq "sum by (action) (rate(registry_storage_action_seconds_count{${sel}}[$__rate_interval]))" "{{action}}")];
              unit = "ops";
            })
            (timeseries {
              title = "Cache hits";
              description = "Share of image lookups answered from memory instead of the disk.";
              targets = [(pq "sum(rate(registry_storage_cache_hits_total{${sel}}[$__rate_interval])) / sum(rate(registry_storage_cache_requests_total{${sel}}[$__rate_interval]) > 0)" "hits")];
              unit = "percentunit";
              min = 0;
              max = 1;
            })
            (timeseries {
              title = "Cache errors";
              description = "Lookups per second where the cache itself failed (the registry then reads the disk).";
              targets = [(pq "sum(rate(registry_storage_cache_errors_total{${sel}}[$__rate_interval]))" "errors")];
              unit = "ops";
            })

            (row "Container" null)
            (timeseries {
              title = "Processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name="registry"}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name="registry"})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit="docker-registry.service"} |~ "(?i)(level=(error|warn|fatal|panic)|\"level\":\"(error|warn|fatal|panic)\")"'')];
            })
          ];
        };
      };
    };
  };
}
