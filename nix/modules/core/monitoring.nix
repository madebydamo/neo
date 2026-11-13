# App metrics and dashboards: neo.monitoring.apps.<service>.
#
# Any service or plugin declares the Prometheus endpoints it exports and the
# Grafana dashboards that show them, next to its own code:
#
#   neo.monitoring.apps.immich = {
#     metrics.api = { container = "immich-server"; port = 8081; };
#     dashboards.overview = {
#       title = "Immich";
#       panels = with lib.neo.grafana; [ (timeseries { … }) ];
#     };
#   };
#
# Only entries whose service is enabled on this machine count. The monitoring
# agent here scrapes the endpoints (labels service, endpoint, host) and sends
# the dashboards to the Monitoring server, which shows them read-only in the
# "Apps" folder. So a dashboard reaches the server even when only another
# machine has the service or plugin; it updates whenever that machine's
# configuration changes. Design: docs/design/monitoring.md.
{...}: {
  flake.modules.nixos.monitoring-apps = {
    config,
    lib,
    ...
  }:
    with lib; let
      outer = config;

      metricType = types.submodule {
        options = {
          container = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = "Docker container that serves the metrics (use plugin.name in plugins). The agent finds its address on `network`.";
          };
          network = mkOption {
            type = types.str;
            default = "internal";
            description = "Docker network the agent reaches the container on.";
          };
          address = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = "host:port for a service that does not run in a container, e.g. 127.0.0.1:9100.";
          };
          port = mkOption {
            type = types.nullOr types.port;
            default = null;
            description = "Metrics port inside the container (with `container`).";
          };
          path = mkOption {
            type = types.str;
            default = "/metrics";
            description = "HTTP path of the Prometheus metrics.";
          };
          scheme = mkOption {
            type = types.enum ["http" "https"];
            default = "http";
            description = "http or https.";
          };
          interval = mkOption {
            type = types.str;
            default = "30s";
            description = "How often the agent scrapes.";
          };
          relabel = mkOption {
            type = types.listOf (types.submodule {
              options = {
                sourceLabels = mkOption {
                  type = types.listOf types.str;
                  default = ["__name__"];
                  description = "Labels whose values (joined with ;) the regex matches.";
                };
                regex = mkOption {
                  type = types.str;
                  default = "(.*)";
                  description = "RE2 regex, anchored on both ends.";
                };
                targetLabel = mkOption {
                  type = types.nullOr types.str;
                  default = null;
                  description = "Label to write (for replace).";
                };
                replacement = mkOption {
                  type = types.str;
                  default = "$1";
                  description = "Value written to targetLabel, with $1… from the regex.";
                };
                action = mkOption {
                  type = types.enum ["replace" "keep" "drop" "labeldrop" "labelkeep"];
                  default = "replace";
                  description = "Prometheus relabel action.";
                };
              };
            });
            default = [];
            example = [
              {
                regex = "myapp_jobs_(.+)_total";
                targetLabel = "task";
              }
              {
                regex = "myapp_jobs_(.+)_total";
                targetLabel = "__name__";
                replacement = "myapp_jobs_total";
              }
            ];
            description = "Metric relabel rules applied after each scrape, e.g. to turn names that carry a value (myapp_jobs_upload_total) into a label.";
          };
        };
      };

      dashboardType = types.submodule {
        options = {
          title = mkOption {
            type = types.str;
            description = "Dashboard title.";
          };
          description = mkOption {
            type = types.str;
            default = "";
            description = "Short description shown in Grafana's dashboard list.";
          };
          panels = mkOption {
            type = types.listOf types.attrs;
            default = [];
            description = "Panels built with lib.neo.grafana; filter queries with lib.neo.grafana.appSelector \"<service>\".";
          };
          variables = mkOption {
            type = types.listOf types.attrs;
            default = [];
            description = "More template variables; `host` (servers running the service) is always added first.";
          };
          time = mkOption {
            type = types.str;
            default = "now-24h";
            description = "Default time range.";
          };
          json = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = "A Grafana dashboard JSON model file instead of panels (e.g. exported from Grafana); uid, title and tags are set by Neo.";
          };
        };
      };

      appType = types.submodule ({name, ...}: {
        options = {
          enabled = mkOption {
            type = types.bool;
            default = outer.neo.services.${name}.enabled or false;
            defaultText = "neo.services.<name>.enabled";
            description = "Whether this machine runs the app (defaults to the service's enabled).";
          };
          metrics = mkOption {
            type = types.attrsOf metricType;
            default = {};
            description = "Prometheus endpoints the app exports, by name (label `endpoint`).";
          };
          dashboards = mkOption {
            type = types.attrsOf dashboardType;
            default = {};
            description = "Grafana dashboards for the app, by id.";
          };
        };
      });

      active = filterAttrs (_: a: a.enabled) config.neo.monitoring.apps;

      # Grafana uids are at most 40 characters.
      uidFor = service: id: let
        full = "app-${service}-${id}";
      in
        if stringLength full <= 40
        then full
        else "app-${substring 0 35 (builtins.hashString "sha256" full)}";

      hostVar = service:
        lib.neo.grafana.queryVar {
          name = "host";
          label = "Server";
          query = ''label_values({service="${service}"}, host)'';
        };

      render = service: id: d: let
        uid = uidFor service id;
        tags = ["neo" "app" service];
        base =
          if d.json != null
          then let
            m = builtins.fromJSON (builtins.readFile d.json);
          in
            m
            // {
              inherit uid tags;
              inherit (d) title;
              id = null;
              templating.list = [(hostVar service)] ++ filter (v: (v.name or "") != "host") (m.templating.list or []);
            }
          else
            lib.neo.grafana.dashboard {
              inherit uid tags;
              inherit (d) title description panels time;
              variables = [(hostVar service)] ++ d.variables;
            };
      in
        base;
    in {
      options.neo.monitoring = {
        apps = mkOption {
          type = types.attrsOf appType;
          default = {};
          description = "Metrics endpoints and Grafana dashboards per app (service name).";
        };

        collecting = mkOption {
          type = types.bool;
          readOnly = true;
          default = config.neo.services.monitoring-agent.enabled or false;
          description = "True when the monitoring agent runs here: apps may then turn on their metrics.";
        };

        # What the agent uses: enabled apps only.
        scrapeTargets = mkOption {
          type = types.listOf types.attrs;
          internal = true;
          readOnly = true;
          default = concatLists (mapAttrsToList (service: app:
            mapAttrsToList (endpoint: m: m // {inherit service endpoint;}) app.metrics)
          active);
          description = "Metric endpoints of enabled apps, flattened (service, endpoint, container/address, …)";
        };

        dashboards = mkOption {
          type = types.attrsOf types.attrs;
          internal = true;
          readOnly = true;
          default = listToAttrs (concatLists (mapAttrsToList (service: app:
            mapAttrsToList (id: d: nameValuePair (uidFor service id) (render service id d)) app.dashboards)
          active));
          description = "Rendered Grafana dashboard models of enabled apps, by uid";
        };
      };

      config.assertions = concatLists (mapAttrsToList (service: app:
        mapAttrsToList (endpoint: m: {
          assertion = (m.container != null) != (m.address != null) && (m.container == null || m.port != null);
          message = "neo.monitoring.apps.${service}.metrics.${endpoint}: set either container and port, or address.";
        })
        app.metrics)
      active);
    };
}
