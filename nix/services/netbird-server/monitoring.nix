# NetBird server metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# netbird-server runs management, signal and relay in one process and serves
# all their metrics on :9090/metrics (combined/cmd: metricsPort defaults to
# 9090, always on, no auth). The port is not published and SWAG only proxies
# :80, so nothing to hide on the public site.
#
# The metrics come from OpenTelemetry's Prometheus exporter (v0.64, the
# UnderscoreEscapingWithSuffixes strategy), so names are rewritten:
#   - counters get _total: management.grpc.login.request.counter
#       → management_grpc_login_request_counter_total
#   - unit "milliseconds" is not a known OTel unit and is appended as-is:
#       management.grpc.sync.request.duration.ms
#       → management_grpc_sync_request_duration_ms_milliseconds_bucket
#   - gauges with unit "1" get _ratio: management.grpc.connected.streams
#       → management_grpc_connected_streams_ratio (renamed back below)
#   - signal_* and relay_* are already Prometheus-style names without units.
# Every series carries otel_scope_* labels and there is a target_info series
# (dropOtelScope). The update channel histograms are internal timings.
{...}: {
  flake.modules.nixos.netbird-server-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.netbird-server;
    sel = lib.neo.grafana.appSelector "netbird-server";
    containers = ''name=~"netbird-(server|dashboard)"'';
  in {
    config = lib.mkIf cfg.enabled {
      neo.monitoring.apps.netbird-server = {
        metrics.main = {
          container = "netbird-server";
          port = 9090;
          relabel =
            lib.neo.monitoring.dropOtelScope
            ++ [
              {
                action = "drop";
                regex = "management_(grpc_)?updatechannel_.*";
              }
              {
                regex = "management_grpc_connected_streams_ratio";
                targetLabel = "__name__";
                replacement = "management_grpc_connected_streams";
              }
            ];
        };

        dashboards.overview = {
          title = "NetBird";
          description = "Devices in your private network, their logins, and the signal and relay services that connect them.";
          time = "now-24h";
          panels = with lib.neo.grafana; [
            (row "Network" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "The NetBird server answers.";
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
              title = "Connected devices";
              w = 4;
              h = 4;
              description = "Devices that are online and receive updates about the network right now.";
              targets = [(pq "sum(management_grpc_connected_streams{${sel}}) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Reachable for direct connections";
              w = 4;
              h = 4;
              description = "Devices connected to the signal service, which helps two devices find each other.";
              targets = [(pq "sum(signal_active_peers{${sel}}) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Using the relay";
              w = 4;
              h = 4;
              description = "Devices that cannot reach each other directly and send their traffic through this server.";
              targets = [(pq "sum(relay_peers{${sel}}) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Logins";
              w = 4;
              h = 4;
              description = "Device logins in the selected time range.";
              targets = [(pq "round(sum(increase(management_grpc_login_request_counter_total{${sel}}[$__range]))) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Blocked logins";
              w = 4;
              h = 4;
              description = "Logins refused in the selected time range, for example from a device you blocked or removed.";
              targets = [(pq "round(sum(increase(management_grpc_login_request_blocked_counter_total{${sel}}[$__range]))) or vector(0)" "")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 1;
                }
              ];
            })
            (timeseries {
              title = "Devices online";
              w = 24;
              h = 7;
              description = "Connected to the server, to the signal service and to the relay.";
              targets = [
                (pq "sum(management_grpc_connected_streams{${sel}})" "connected")
                (pq "sum(signal_active_peers{${sel}})" "signal")
                (pq "sum(relay_peers{${sel}})" "relay")
              ];
            })

            (row "Logins and updates" null)
            (bars {
              title = "Logins";
              description = "Device logins, and those that were refused.";
              targets = [
                (pq "sum(increase(management_grpc_login_request_counter_total{${sel}}[$__interval]))" "logins")
                (pq "sum(increase(management_grpc_login_request_blocked_counter_total{${sel}}[$__interval]))" "blocked")
              ];
            })
            (timeseries {
              title = "Updates requested";
              description = "How often devices ask the server for the current state of the network.";
              targets = [
                (pq "sum(rate(management_grpc_sync_request_counter_total{${sel}}[$__rate_interval]))" "updates")
                (pq "sum(rate(management_grpc_sync_request_blocked_counter_total{${sel}}[$__rate_interval]))" "blocked")
              ];
              unit = "reqps";
            })
            (timeseries {
              title = "Login and update time (95%)";
              description = "How long the server takes to answer a login or an update; 95 of 100 answers are faster.";
              targets = [
                (pq "histogram_quantile(0.95, sum by (le) (rate(management_grpc_login_request_duration_ms_milliseconds_bucket{${sel}}[$__rate_interval])))" "login")
                (pq "histogram_quantile(0.95, sum by (le) (rate(management_grpc_sync_request_duration_ms_milliseconds_bucket{${sel}}[$__rate_interval])))" "update")
              ];
              unit = "ms";
            })
            (bars {
              title = "Errors on the website and API";
              description = "Answers of the NetBird dashboard's server side that failed, by status code.";
              targets = [(pq ''sum by (code) (increase(management_http_response_counter_total{${sel}, code=~"5.."}[$__interval]))'' "{{code}}")];
            })

            (row "Signal and relay" null)
            (timeseries {
              title = "Messages between devices";
              description = "Messages the signal service passes on so two devices can connect, and those it could not deliver.";
              targets = [
                (pq "sum(rate(signal_messages_forwarded_total{${sel}}[$__rate_interval]))" "passed on")
                (pq "sum(rate(signal_message_forward_failures_total{${sel}}[$__rate_interval]))" "failed")
              ];
              unit = "ops";
            })
            (timeseries {
              title = "Message time (95%)";
              description = "How long the signal service takes to pass on a message.";
              targets = [(pq "histogram_quantile(0.95, sum by (le) (rate(signal_message_forward_latency_milliseconds_bucket{${sel}}[$__rate_interval])))" "message")];
              unit = "ms";
            })
            (timeseries {
              title = "Relay traffic";
              description = "Data the relay sends to and receives from devices.";
              targets = [
                (pq "sum(rate(relay_transfer_sent_bytes_total{${sel}}[$__rate_interval]))" "sent")
                (pq "sum(rate(relay_transfer_received_bytes_total{${sel}}[$__rate_interval]))" "received")
              ];
              unit = "Bps";
            })
            (timeseries {
              title = "Devices on the relay";
              description = "Active devices send data, idle ones only stay connected.";
              targets = [
                (pq "sum(relay_peers_active{${sel}})" "active")
                (pq "sum(relay_peers_idle{${sel}})" "idle")
              ];
            })

            (row "Login provider" null)
            (bars {
              title = "Login provider errors";
              w = 24;
              h = 6;
              description = "Failed requests to an external login provider. Stays empty with NetBird's built-in login.";
              targets = [
                (pq "sum(increase(management_idp_request_error_counter_total{${sel}}[$__interval]))" "request failed")
                (pq "sum(increase(management_idp_request_status_error_counter_total{${sel}}[$__interval]))" "error answer")
              ];
            })

            (row "Processes and containers" null)
            (timeseries {
              title = "Processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", ${containers}}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", ${containers}})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-netbird-.*"} |~ "(?i)(error|warn|exception|fail)"'')];
            })
          ];
        };
      };
    };
  };
}
