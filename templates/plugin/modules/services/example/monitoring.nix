# Example metrics, dashboard and alert rule (Monitoring). Shown in Grafana's "Apps" folder,
# read-only, sent by the monitoring agent of whichever machine runs the plugin.
# Without an exporter the dashboard uses what every app gets anyway: container
# numbers, the container's log and SWAG's request log. An app with a Prometheus
# endpoint declares it under `metrics` (see the commented block) and queries
# its own metric names filtered by appSelector.
# Reference: $NEO/docs/LIB.md → Metrics and dashboards.
{...}: {
  neoPlugin.modules.example-monitoring = {
    config,
    lib,
    plugin,
    ...
  }: let
    name = plugin.name "example";
    cfg = config.neo.services.${name};
  in {
    config = lib.mkIf cfg.enabled {
      neo.monitoring.apps.${name} = {
        # metrics.main = {
        #   container = name;   # scraped over its own network
        #   port = 9113;
        #   path = "/metrics";
        #   auth = "bearer";    # token: the generated scrapeToken, see below
        # };
        # hidePaths = [ "/metrics" ];  # when metrics are on the public port
        # neo.core.secrets.containerEnv.${name} = lib.mkIf config.neo.monitoring.collecting {
        #   METRICS_TOKEN = config.neo.monitoring.apps.${name}.scrapeToken;
        # };
        # Turn the exporter on only while an agent scrapes it:
        # virtualisation.oci-containers.containers.${name}.environment =
        #   lib.mkIf config.neo.monitoring.collecting { METRICS_ENABLED = "true"; };

        dashboards.overview = {
          title = "Example${plugin.label}";
          panels = with lib.neo.grafana; [
            (timeseries {
              title = "Requests";
              targets = [(pq ''sum by (status_class) (rate(neo_http_requests_total{host=~"$host", service="${name}"}[$__rate_interval]))'' "{{status_class}}")];
              unit = "reqps";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq ''container_memory_working_set_bytes{host=~"$host", name="${name}"}'' "{{host}}")];
              unit = "bytes";
            })
            (logs {
              title = "Log";
              w = 24;
              targets = [(lq ''{job="journal", host=~"$host", unit="docker-${name}.service"}'')];
            })
          ];
        };

        # Alert on what the owner must act on, in plain words. Grafana's Apps
        # folder, sent to the assistant like Neo's own alerts. With metrics:
        # expr = ''max by (host) (time() - example_last_sync_timestamp_seconds{${lib.neo.grafana.alertSelector name}})'';
        alerts.errors = {
          title = "Example${plugin.label} logs errors";
          datasource = "loki";
          expr = ''sum by (host) (count_over_time({job="journal", unit="docker-${name}.service"} |~ `(?i)\berror\b` [10m]))'';
          above = 5;
          for = "0s";
          summary = "Example${plugin.label} on {{ $labels.host }} logged {{ printf \"%.0f\" $values.B.Value }} errors in 10 minutes.";
        };
      };
    };
  };
}
