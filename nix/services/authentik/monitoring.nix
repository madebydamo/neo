# Authentik metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# Server and worker both serve Prometheus metrics on :9300 (any path), always on,
# no auth, reachable only on the authentik Docker network. Names checked against
# goauthentik/authentik version/2026.8.5:
#   - Rust front (src/server, src/metrics): authentik_main_request_duration_seconds{dest}.
#     metrics-exporter-prometheus without buckets renders histograms as a
#     summary (quantile label, _sum, _count), so no histogram_quantile on it.
#   - Python (multiprocess prometheus_client, appended by the Rust handler):
#     authentik_flows_{plan_time,execution_stage_time,stage_time}_*,
#     authentik_policies_execution_time_*, authentik_outposts_connected,
#     authentik_tasks_{total,errors_total,retries_total,in_progress,queued,workers},
#     authentik_tasks_duration_milliseconds_*, django_http_* (django-prometheus 2.5.0,
#     server only).
#   - Gauges filled from the database (workers, queued tasks) are set by every
#     process that is scraped, so server and worker report the same value:
#     the queries take max over the two scrapes.
#
# authentik_policies_engine_time_total_seconds carries obj_pk (one series set per
# flow, application, binding, …). Dropping only that label would make several
# series collide in one scrape, which the remote write rejects batch-wide, so the
# whole metric is dropped; policy time comes from authentik_policies_execution_time.
# authentik_admin_workers is the old name of authentik_tasks_workers.
#
# There is no login counter. Every audit event is logged as JSON by the server:
#   {"action": "login", "client_ip": "…", "context": {…}, "event": "Created Event", "user": {"username": …}, …}
# (authentik/events/models.py Event.save, JSONRenderer). The login panels read it from Loki.
{...}: {
  flake.modules.nixos.authentik-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.authentik;
    sel = lib.neo.grafana.appSelector "authentik";

    relabel = [
      {
        action = "drop";
        regex = "authentik_policies_engine_time_total_seconds_.*|authentik_admin_workers";
      }
    ];

    journal = ''{job="journal", host=~"$host", unit=~"docker-authentik.*"}'';
    # Audit events with their action extracted; robust against the rest of the JSON.
    events = ''${journal} |= "Created Event" | json action="action"'';
    eventCount = actions: range: ''sum(count_over_time(${events} | action=~"${actions}" [${range}]))'';

    workers = "max(sum by (scrape) (authentik_tasks_workers{${sel}}))";
  in {
    config = lib.mkIf cfg.enabled {
      neo.monitoring.apps.authentik = {
        metrics = {
          server = {
            container = "authentik";
            port = 9300;
            inherit relabel;
          };
          worker = {
            container = "authentik-worker";
            port = 9300;
            inherit relabel;
          };
        };

        dashboards.overview = {
          title = "Authentik";
          description = "Sign-ins, login flows, background tasks and the server and worker of Authentik.";
          time = "now-7d";
          panels = with lib.neo.grafana; [
            (row "Overview" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "The Authentik server and its background worker both answer.";
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
              title = "Sign-ins";
              w = 4;
              h = 4;
              description = "Successful sign-ins in the selected time range.";
              datasource = loki;
              targets = [(lq "${eventCount "login" "$__range"} or vector(0)")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Failed sign-ins";
              w = 4;
              h = 4;
              description = "Wrong passwords or codes in the selected time range. Many of them can mean someone is guessing.";
              datasource = loki;
              targets = [(lq "${eventCount "login_failed" "$__range"} or vector(0)")];
              thresholds = thresholdsGreenRed [
                {
                  color = "orange";
                  value = 10;
                }
                {
                  color = "red";
                  value = 100;
                }
              ];
            })
            (stat {
              title = "Workers connected";
              w = 4;
              h = 4;
              description = "Background workers that checked in during the last minute. Without one, emails, outposts and clean-up stop.";
              targets = [(pq "${workers} or vector(0)" "")];
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
              title = "Outposts connected";
              w = 4;
              h = 4;
              description = "Separate outposts (proxy, LDAP, RADIUS) connected right now. Zero is normal when you run none.";
              targets = [(pq "sum(max by (outpost, uid) (authentik_outposts_connected{${sel}})) or vector(0)" "")];
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })
            (stat {
              title = "Memory";
              w = 4;
              h = 4;
              description = "All Authentik containers together.";
              targets = [(pq ''sum(container_memory_working_set_bytes{host=~"$host", name=~"authentik.*"})'' "")];
              unit = "bytes";
              options.colorMode = "value";
              thresholds = thresholdsGreenRed [];
            })

            (row "Sign-ins" null)
            (bars {
              title = "Sign-ins and sign-outs";
              description = "Successful sign-ins, failed attempts and sign-outs over time.";
              datasource = loki;
              targets = [(lqLegend ''sum by (action) (count_over_time(${events} | action=~"login|login_failed|logout" [$__interval]))'' "{{action}}")];
            })
            (logs {
              title = "Sign-in events";
              description = "Who signed in or failed to, and from which address.";
              targets = [(lq ''${events} | action=~"login|login_failed" | json user="user.username", tried="context.username", ip="client_ip" | line_format "{{.action}}: {{if .tried}}{{.tried}}{{else}}{{.user}}{{end}} from {{.ip}}"'')];
            })

            (row "Login flows" null)
            (bars {
              title = "Flows started";
              description = "How often each flow (sign-in, enrollment, password reset, …) was started.";
              targets = [(pq ''sum by (flow_slug) (increase(authentik_flows_plan_time_count{${sel}}[$__interval]))'' "{{flow_slug}}")];
            })
            (timeseries {
              title = "Time to start a flow (95%)";
              description = "How long Authentik needs to prepare a flow before the first page shows.";
              targets = [(pq ''histogram_quantile(0.95, sum by (flow_slug, le) (rate(authentik_flows_plan_time_bucket{${sel}}[$__rate_interval])))'' "{{flow_slug}}")];
              unit = "s";
            })
            (timeseries {
              title = "Slowest flow steps (95%)";
              description = "The ten slowest kinds of steps (password, two-factor code, consent, …).";
              targets = [(pq ''topk(10, histogram_quantile(0.95, sum by (stage_type, le) (rate(authentik_flows_execution_stage_time_bucket{${sel}}[$__rate_interval]))))'' "{{stage_type}}")];
              unit = "s";
            })
            (timeseries {
              title = "Slowest policies (95%)";
              description = "The ten slowest access rules Authentik checks, by what they are attached to.";
              targets = [(pq ''topk(10, histogram_quantile(0.95, sum by (binding_target_type, binding_target_name, le) (rate(authentik_policies_execution_time_bucket{${sel}}[$__rate_interval]))))'' "{{binding_target_type}} {{binding_target_name}}")];
              unit = "s";
            })

            (row "Background tasks" null)
            (bars {
              title = "Tasks done";
              description = "Background tasks (emails, outpost updates, clean-up, syncs) the worker ran.";
              targets = [(pq ''topk(10, sum by (actor_name) (increase(authentik_tasks_total{${sel}}[$__interval])))'' "{{actor_name}}")];
            })
            (bars {
              title = "Tasks failed";
              targets = [(pq ''sum by (actor_name) (increase(authentik_tasks_errors_total{${sel}}[$__interval]))'' "{{actor_name}}")];
            })
            (timeseries {
              title = "Tasks waiting and running";
              targets = [
                (pq ''sum(max by (queue_name, actor_name) (authentik_tasks_queued{${sel}})) or vector(0)'' "waiting")
                (pq ''sum(authentik_tasks_in_progress{${sel}}) or vector(0)'' "running")
                (pq "${workers} or vector(0)" "workers")
              ];
            })
            (timeseries {
              title = "Task time (95%)";
              description = "The ten slowest kinds of background task.";
              targets = [(pq ''topk(10, histogram_quantile(0.95, sum by (actor_name, le) (rate(authentik_tasks_duration_milliseconds_bucket{${sel}}[$__rate_interval]))))'' "{{actor_name}}")];
              unit = "ms";
            })

            (row "Requests" null)
            (timeseries {
              title = "Requests";
              description = "Requests per second, to Authentik itself (core) and to apps behind its built-in outpost.";
              targets = [(pq ''sum by (dest) (rate(authentik_main_request_duration_seconds_count{${sel}}[$__rate_interval]))'' "{{dest}}")];
              unit = "reqps";
            })
            (timeseries {
              title = "Response time";
              description = "How long answers take: typical (50%) and slow (95%) requests over the last minute.";
              targets = [
                (pq ''max by (dest) (authentik_main_request_duration_seconds{${sel}, quantile="0.5"})'' "{{dest}} 50%")
                (pq ''max by (dest) (authentik_main_request_duration_seconds{${sel}, quantile="0.95"})'' "{{dest}} 95%")
              ];
              unit = "s";
            })
            (timeseries {
              title = "Answers by status";
              description = "Answers per second by HTTP status. Many 4xx or 5xx point at a problem.";
              targets = [(pq ''sum by (status) (rate(django_http_responses_total_by_status_total{${sel}}[$__rate_interval]))'' "{{status}}")];
              unit = "reqps";
            })
            (timeseries {
              title = "Slowest pages (95%)";
              description = "The ten slowest parts of Authentik (API, flows, admin pages).";
              targets = [(pq ''topk(10, histogram_quantile(0.95, sum by (view, le) (rate(django_http_requests_latency_seconds_by_view_method_bucket{${sel}}[$__rate_interval]))))'' "{{view}}")];
              unit = "s";
            })

            (row "Processes and containers" null)
            (timeseries {
              title = "Processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name=~"authentik.*"}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name=~"authentik.*"})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''${journal} |~ "(?i)(error|warn|exception|fail)" != "Created Event"'')];
            })
          ];
        };
      };
    };
  };
}
