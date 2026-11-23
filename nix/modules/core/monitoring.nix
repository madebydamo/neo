# App metrics and dashboards: neo.monitoring.apps.<service>.
#
# Any service or plugin declares the Prometheus endpoints it exports, the
# Grafana dashboards that show them and alert rules, next to its own code:
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
# agent here scrapes the endpoints (labels service, scrape, host) and sends
# the dashboards and alert rules to the Monitoring server, which shows them
# read-only in the "Apps" folder. So a dashboard reaches the server even when only another
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

      generatedKey = lib.neo.secrets.mkPlaceholder "generated/metrics";

      metricType = service:
        types.submodule {
          options = {
            container = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Docker container that serves the metrics (use plugin.name in plugins). The agent finds its address on `network`.";
            };
            network = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Docker network the agent reaches the container on. Default: the container's first network (its service network, or internal).";
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
            params = mkOption {
              type = types.attrsOf (types.listOf types.str);
              default = {};
              example = {target = ["db:5432"];};
              description = "URL query parameters of the scrape (multi-target exporters).";
            };
            auth = mkOption {
              type = types.enum ["none" "bearer" "basic"];
              default = "none";
              description = "How the agent authenticates: none, bearer (Authorization: Bearer <token>) or basic (username and token as password).";
            };
            username = mkOption {
              type = types.str;
              default = "neo-metrics";
              description = "User name for basic auth.";
            };
            token = mkOption {
              type = types.str;
              default = outer.neo.monitoring.apps.${service}.scrapeToken;
              defaultText = "neo.monitoring.apps.<service>.scrapeToken";
              description = "Bearer token or basic-auth password; may be a secret placeholder (it only reaches the agent's rendered environment).";
            };
            headers = mkOption {
              type = types.attrsOf types.str;
              default = {};
              example = {"X-Forwarded-Proto" = "https";};
              description = "Extra request headers (not secret: they are written into the agent's config).";
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

      alertType = types.submodule {
        options = {
          title = mkOption {
            type = types.str;
            description = "Alert name, shown in Grafana and in the assistant's message (e.g. \"GPS feed stale\").";
          };
          expr = mkOption {
            type = types.str;
            example = ''max by (host) (time() - myapp_last_poll_timestamp_seconds{service="myapp"})'';
            description = "Query evaluated every minute (PromQL, or a LogQL metric query with datasource = \"loki\"). Each series it returns is one alert; keep `host` and the labels the summary names. Filter with lib.neo.grafana.alertSelector \"<service>\".";
          };
          datasource = mkOption {
            type = types.enum ["prometheus" "loki"];
            default = "prometheus";
            description = "prometheus for metrics, loki for a metric query over logs (count_over_time, rate, …).";
          };
          above = mkOption {
            type = types.nullOr types.number;
            default = null;
            description = "Fire when the value is above this. Default (neither above nor below): above 0.";
          };
          below = mkOption {
            type = types.nullOr types.number;
            default = null;
            description = "Fire when the value is below this instead.";
          };
          for = mkOption {
            type = types.str;
            default = "5m";
            description = "How long the condition must hold before the alert fires (\"0s\" at once).";
          };
          severity = mkOption {
            type = types.enum ["info" "warning" "critical"];
            default = "warning";
            description = "Label severity: critical for outages, warning for degradation, info for notices.";
          };
          summary = mkOption {
            type = types.str;
            example = "No GPS position from {{ $labels.host }} for {{ printf \"%.0f\" $values.B.Value }} seconds.";
            description = "Message for the owner, plain words: what is wrong and where. Go template: {{ $labels.<name> }}, {{ $values.B.Value }}.";
          };
          noData = mkOption {
            type = types.enum ["ok" "alerting" "nodata"];
            default = "ok";
            description = "State when the query returns nothing: ok (default), alerting (silence is the problem, e.g. a feed that stopped) or nodata.";
          };
          labels = mkOption {
            type = types.attrsOf types.str;
            default = {};
            description = "Extra labels on the alert (service and severity are set).";
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
            type = types.attrsOf (metricType name);
            default = {};
            description = "Prometheus endpoints the app exports, by name (label `scrape`).";
          };
          scrapeToken = mkOption {
            type = types.str;
            readOnly = true;
            default = lib.neo.secrets.sha256 "neo-metrics:${name}:${generatedKey}";
            defaultText = "a 64-hex token derived from this machine's generated metrics key";
            description = "Secret token for this app's metrics, one per app, generated on the machine. Give it to the app (containerEnv, secrets templates), never to environment or the store.";
          };
          uptimePath = mkOption {
            type = types.nullOr types.str;
            default = null;
            example = "/api/health";
            description = "Path the uptime check calls instead of /, answered by the app itself without a login (it must be in auth.publicPaths when the app is behind the login). Only a 2xx answer counts as up. Null checks /, where a login redirect also counts as up, so the check only sees the login page.";
          };
          hidePaths = mkOption {
            type = types.listOf types.str;
            default = [];
            example = ["/metrics" "^~ /admin/metrics/"];
            description = "Paths SWAG answers with 404 on this app's sites (exact paths, or `^~ /prefix`), so metrics served on the public port never reach the Internet. Applies whether or not anything scrapes them.";
          };
          dashboards = mkOption {
            type = types.attrsOf dashboardType;
            default = {};
            description = "Grafana dashboards for the app, by id.";
          };
          alerts = mkOption {
            type = types.attrsOf alertType;
            default = {};
            description = "Grafana alert rules for the app, by id. They land read-only in the Apps folder, rule group <service>, and go to the assistant like Neo's own alerts.";
          };
        };
      });

      active = filterAttrs (_: a: a.enabled) config.neo.monitoring.apps;

      # Grafana folder of app dashboards and alert rules ("Apps").
      appsFolder = "neo-apps";

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

        # Node exporter textfile directory: *.prom files any unit writes here
        # (backup status, the Monitoring server's list of machines) are sent
        # by the agent with the other numbers. Outside the store, so a unit
        # can update it at run time; outside appdata, because it describes
        # this machine, not app data (a restore must not bring back old values).
        textfileDir = mkOption {
          type = types.str;
          readOnly = true;
          internal = true;
          default = "/var/lib/neo/textfile";
          description = "Folder of extra numbers the monitoring agent reports.";
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
          default = let
            containers = config.virtualisation.oci-containers.containers;
            networkOf = m:
              if m.network != null || m.container == null
              then m.network
              else let
                nets = optionals (containers ? ${m.container}) (lib.neo.docker.containerNetworks containers.${m.container});
              in
                if nets == []
                then lib.neo.docker.sharedNetwork
                else head nets;
            # A VPN-routed container lives in gluetun's namespace: scrape the
            # vpn container on the service's network, where gluetun answers
            # for it (vpn/default.nix).
            routed = lib.neo.vpnRoutedContainers config;
            viaVpn = service: m:
              if m.container != null && elem m.container routed
              then
                m
                // {
                  container = "vpn";
                  network =
                    if m.network != null
                    then m.network
                    else lib.neo.docker.serviceNetwork service;
                }
              else m;
          in
            concatLists (mapAttrsToList (service: app:
              mapAttrsToList (scrape: m0: let
                m = viaVpn service m0;
              in
                m
                // {
                  inherit service scrape;
                  network = networkOf m;
                })
              app.metrics)
            active);
          description = "Metric endpoints of enabled apps, flattened (service, scrape, container/address, auth, token, …)";
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

        # One Grafana rule group per app (folder Apps, uid neo-apps). The agent
        # sends them with the dashboards; the server's grafana-app-alerts unit
        # puts them into Grafana (monitoring/default.nix).
        alertGroups = mkOption {
          type = types.attrsOf types.attrs;
          internal = true;
          readOnly = true;
          default = mapAttrs (service: app: {
            title = service;
            folderUid = appsFolder;
            interval = 60;
            rules = mapAttrsToList (id: a:
              lib.neo.grafana.alertRule {
                uid = uidFor service id;
                inherit (a) title expr datasource above below for severity summary noData;
                labels = a.labels // {inherit service;};
              }
              // {
                folderUID = appsFolder;
                ruleGroup = service;
                orgID = 1;
              })
            app.alerts;
          }) (filterAttrs (_: app: app.alerts != {}) active);
          description = "Rendered Grafana alert rule groups of enabled apps, by service";
        };
      };

      config.neo.core.secrets.generated.metrics = {};

      # Writers (backup, monitoring-expected-hosts as root; units of the neo
      # user) write x.prom.tmp, then mv. The agent mounts it read-only.
      config.systemd.tmpfiles.rules = ["d ${config.neo.monitoring.textfileDir} 0775 root ${toString config.neo.core.gid} -"];

      config.assertions =
        concatLists (mapAttrsToList (service: app:
          mapAttrsToList (scrape: m: {
            assertion = (m.container != null) != (m.address != null) && (m.container == null || m.port != null);
            message = "neo.monitoring.apps.${service}.metrics.${scrape}: set either container and port, or address.";
          })
          app.metrics)
        active)
        ++ concatLists (mapAttrsToList (service: app:
          mapAttrsToList (id: a: {
            assertion = a.above == null || a.below == null;
            message = "neo.monitoring.apps.${service}.alerts.${id}: set above or below, not both.";
          })
          app.alerts)
        active);
    };
}
