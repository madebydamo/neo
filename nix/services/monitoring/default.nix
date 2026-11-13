# Monitoring server: Grafana + Prometheus + Loki on their own Docker network
# (`monitoring`, shared with SWAG only).
#
# Agents (monitoring-agent) write logs to Loki's push API and numbers to
# Prometheus' remote-write receiver: the one on this machine over loopback
# ports, others through SWAG at /ingest/… with the ingest token (swag.nix).
# Datasources and the Hermes contact point are provisioned from the store;
# dashboards, alert rules and the notification policy are seeded once by
# grafana-seed and then belong to the users. Everyone tinyauth lets through
# signs in via the auth proxy header as admin; Hermes and the seed use
# `docker exec grafana curl -H 'Remote-User: …' http://127.0.0.1:3000`.
{...}: {
  flake.modules.nixos.monitoring = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.monitoring;
      hermes = config.neo.services.hermes;
      uidgid = "${toString config.neo.core.uid}:${toString config.neo.core.gid}";
      grafanaDir = "${cfg.appdata}/grafana";
      prometheusDir = "${cfg.appdata}/prometheus";
      lokiDir = "${cfg.appdata}/loki";

      # Own network, shared with SWAG only: Loki and Prometheus have no
      # authentication and Grafana trusts the Remote-User header, so no other
      # container may reach them.
      network = "monitoring";

      alertsOn = cfg.assistantAlerts && hermes.enabled;
      hermesSecretPath = "/etc/grafana/neo/hermes-webhook.secret";
      # Hermes delivers to Telegram when a bot is set up, else only logs.
      deliver =
        if (hermes.telegramBotToken or null) != null && hermes.telegramAllowedUserId != []
        then "telegram"
        else "log";

      grafanaLib = import ./_grafana.nix {inherit lib;};
      yaml = pkgs.formats.yaml {};

      datasources = yaml.generate "datasources.yaml" {
        apiVersion = 1;
        deleteDatasources = [];
        datasources = [
          {
            name = "Prometheus";
            uid = "prometheus";
            type = "prometheus";
            access = "proxy";
            url = "http://prometheus:9090";
            isDefault = true;
            editable = false;
            jsonData.timeInterval = "30s";
          }
          {
            name = "Loki";
            uid = "loki";
            type = "loki";
            access = "proxy";
            url = "http://loki:3100";
            editable = false;
            jsonData.maxLines = 5000;
          }
        ];
      };

      # Seeds: dashboards, folders, alert rules and the notification policy are
      # imported once through the API (grafana-seed) as ordinary, editable
      # objects. An object whose uid exists is never touched again, so edits by
      # people or Hermes stay; deleting one brings Neo's version back on the
      # next start of Grafana.
      json = name: value: pkgs.writeText name (builtins.toJSON value);
      seedFiles =
        mapAttrsToList (uid: f: {
          name = "folder-${uid}.json";
          value = {
            inherit uid;
            inherit (f) title;
          };
        })
        grafanaLib.folders
        ++ mapAttrsToList (name: d: {
          name = "dashboard-${name}.json";
          value = {
            inherit (d) dashboard;
            folderUid = d.folder;
            overwrite = false;
            message = "Seeded by Neo";
          };
        })
        grafanaLib.dashboards
        ++ map (r: {
          name = "rule-${r.uid}.json";
          value = r;
        })
        grafanaLib.alertRules
        ++ optional alertsOn {
          name = "policy.json";
          value = {
            receiver = "Assistant";
            group_by = ["grafana_folder" "alertname" "host"];
            group_wait = "30s";
            group_interval = "5m";
            repeat_interval = "12h";
          };
        };
      # Real files: the container sees no /nix/store, so no symlinks.
      seedDir = pkgs.runCommand "neo-grafana-seed" {} (''
          mkdir $out
        ''
        + concatMapStrings (f: ''
          cp ${json f.name f.value} $out/${f.name}
        '')
        seedFiles);

      seedScript = let
        api = "http://127.0.0.1:3000";
        ensure = getPath: postPath: file: extra: ''
          code=$(g ${api}${getPath})
          if [ "$code" = 404 ]; then
            code=$(g -X POST ${extra} -d @/var/lib/neo-seed/${file} ${api}${postPath})
            case "$code" in 2*) echo "seeded ${file}" ;; *) echo "seeding ${file} failed: HTTP $code" >&2; exit 1 ;; esac
          elif [ "''${code#2}" = "$code" ]; then
            echo "checking ${getPath} failed: HTTP $code" >&2; exit 1
          fi
        '';
      in
        ''
          # Inside the container, as the Grafana user "neo" (auth proxy header;
          # only SWAG and this host reach Grafana).
          g() { docker exec grafana curl -sS -o /dev/null -w '%{http_code}' -H 'Remote-User: neo' -H 'Content-Type: application/json' "$@"; }
          [ "$(g ${api}/api/health)" = 200 ]
        ''
        + concatStrings (mapAttrsToList (uid: _: ensure "/api/folders/${uid}" "/api/folders" "folder-${uid}.json" "") grafanaLib.folders)
        + concatStrings (mapAttrsToList (name: d: ensure "/api/dashboards/uid/${d.dashboard.uid}" "/api/dashboards/db" "dashboard-${name}.json" "") grafanaLib.dashboards)
        + concatMapStrings (r: ensure "/api/v1/provisioning/alert-rules/${r.uid}" "/api/v1/provisioning/alert-rules" "rule-${r.uid}.json" "-H 'X-Disable-Provenance: true'") grafanaLib.alertRules
        + ''
          # Open the overview on the home page unless someone picked another one.
          home=$(docker exec grafana curl -sS -H 'Remote-User: neo' ${api}/api/org/preferences | jq -r '.homeDashboardUID // ""')
          if [ -z "$home" ]; then
            [ "$(g -X PATCH -d '{"homeDashboardUID":"neo-overview"}' ${api}/api/org/preferences)" = 200 ]
          fi
        ''
        + optionalString alertsOn ''
          # Point the default policy at the assistant unless someone changed it.
          root=$(docker exec grafana curl -sS -H 'Remote-User: neo' ${api}/api/v1/provisioning/policies | jq -r .receiver)
          # Untouched default: "empty" (Grafana 12+) or "grafana-default-email".
          if [ "$root" = empty ] || [ "$root" = grafana-default-email ]; then
            code=$(g -X PUT -H 'X-Disable-Provenance: true' -d @/var/lib/neo-seed/policy.json ${api}/api/v1/provisioning/policies)
            case "$code" in 2*) echo "seeded notification policy" ;; *) echo "policy failed: HTTP $code" >&2; exit 1 ;; esac
          fi
        '';

      # Signed like Hermes' generic V1 webhook: hex HMAC-SHA256 of the body.
      # title/message are what Hermes forwards to Telegram (route prompt).
      contactPoints = yaml.generate "contact-points.yaml" {
        apiVersion = 1;
        contactPoints = [
          {
            orgId = 1;
            name = "Assistant";
            receivers = [
              {
                uid = "neo-hermes";
                type = "webhook";
                disableResolveMessage = false;
                settings = {
                  url = "http://host.docker.internal:${toString hermes.webhook.port}/webhooks/grafana";
                  httpMethod = "POST";
                  hmacConfig = {
                    secret = "$__file{${hermesSecretPath}}";
                    header = "X-Webhook-Signature";
                  };
                  title = ''{{ if eq .Status "firing" }}🔴 {{ .CommonLabels.alertname }}{{ else }}✅ Resolved: {{ .CommonLabels.alertname }}{{ end }}'';
                  message = ''{{ range .Alerts }}{{ if eq .Status "resolved" }}(resolved) {{ end }}{{ or .Annotations.summary .Labels.alertname }}{{ "\n" }}{{ end }}{{ .ExternalURL }}'';
                };
              }
            ];
          }
        ];
      };

      provisioning = pkgs.runCommand "neo-grafana-provisioning" {} ''
        mkdir -p $out/datasources $out/dashboards $out/alerting $out/plugins $out/notifiers
        cp ${datasources} $out/datasources/neo.yaml
        ${optionalString alertsOn "cp ${contactPoints} $out/alerting/contact-points.yaml"}
      '';

      lokiConfig = yaml.generate "loki.yaml" {
        auth_enabled = false;
        server = {
          http_listen_port = 3100;
          grpc_listen_port = 9096;
          log_level = "warn";
        };
        common = {
          instance_addr = "127.0.0.1";
          path_prefix = "/loki";
          storage.filesystem = {
            chunks_directory = "/loki/chunks";
            rules_directory = "/loki/rules";
          };
          replication_factor = 1;
          ring.kvstore.store = "inmemory";
        };
        schema_config.configs = [
          {
            from = "2024-01-01";
            store = "tsdb";
            object_store = "filesystem";
            schema = "v13";
            index = {
              prefix = "index_";
              period = "24h";
            };
          }
        ];
        limits_config = {
          retention_period = "${toString (cfg.logRetentionDays * 24)}h";
          allow_structured_metadata = true;
          volume_enabled = true;
          # "by (path)" / "by (ip)" panels group over many values.
          max_query_series = 10000;
          reject_old_samples = true;
          reject_old_samples_max_age = "168h";
          ingestion_rate_mb = 16;
          ingestion_burst_size_mb = 32;
        };
        compactor = {
          working_directory = "/loki/compactor";
          retention_enabled = true;
          delete_request_store = "filesystem";
        };
        pattern_ingester.enabled = true;
        analytics.reporting_enabled = false;
      };

      prometheusConfig = yaml.generate "prometheus.yml" {
        global = {
          scrape_interval = "30s";
          evaluation_interval = "30s";
        };
        scrape_configs = [];
      };
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = config.neo.services.swag.enabled;
            message = "neo.services.monitoring needs SWAG (services.swag.enabled) for Grafana and the ingest endpoints.";
          }
        ];

        systemd.services.docker-grafana.preStart = lib.neo.mkEnsureDirs config [cfg.appdata grafanaDir];
        systemd.services.docker-prometheus.preStart = lib.neo.mkEnsureDirs config [cfg.appdata prometheusDir];
        systemd.services.docker-loki.preStart = lib.neo.mkEnsureDirs config [cfg.appdata lokiDir];

        virtualisation.oci-containers.containers = {
          grafana = {
            image = cfg.containers.grafana;
            user = uidgid;
            volumes =
              [
                "${grafanaDir}:/var/lib/grafana"
                "${provisioning}:/etc/grafana/provisioning:ro"
                "${seedDir}:/var/lib/neo-seed:ro"
              ]
              ++ optional alertsOn "${hermes.webhook.secretFile}:${hermesSecretPath}:ro";
            environment = {
              TZ = config.neo.core.timeZone;
              GF_SERVER_ROOT_URL = lib.neo.publicUrl config cfg;
              GF_PATHS_DATA = "/var/lib/grafana";
              GF_PATHS_PROVISIONING = "/etc/grafana/provisioning";
              # Sign-in through tinyauth: SWAG sets Remote-User after the auth
              # subrequest and clears it when login is off (swag.nix).
              GF_AUTH_PROXY_ENABLED = "true";
              GF_AUTH_PROXY_HEADER_NAME = "Remote-User";
              GF_AUTH_PROXY_HEADER_PROPERTY = "username";
              GF_AUTH_PROXY_HEADERS = "Email:Remote-Email Name:Remote-Name";
              GF_AUTH_PROXY_AUTO_SIGN_UP = "true";
              GF_AUTH_PROXY_ENABLE_LOGIN_TOKEN = "false";
              GF_AUTH_DISABLE_LOGIN_FORM = "true";
              GF_AUTH_DISABLE_SIGNOUT_MENU = "true";
              GF_USERS_ALLOW_SIGN_UP = "false";
              GF_USERS_AUTO_ASSIGN_ORG_ROLE = "Admin";
              GF_SECURITY_DISABLE_INITIAL_ADMIN_CREATION = "true";
              GF_ANALYTICS_REPORTING_ENABLED = "false";
              GF_ANALYTICS_CHECK_FOR_UPDATES = "false";
              GF_ANALYTICS_CHECK_FOR_PLUGIN_UPDATES = "false";
              GF_NEWS_NEWS_FEED_ENABLED = "false";
              GF_UNIFIED_ALERTING_ENABLED = "true";
            };
            networks = [network];
            extraOptions = optional alertsOn "--add-host=host.docker.internal:host-gateway";
          };

          prometheus = {
            image = cfg.containers.prometheus;
            user = uidgid;
            cmd = [
              "--config.file=/etc/prometheus/prometheus.yml"
              "--storage.tsdb.path=/prometheus"
              "--storage.tsdb.retention.time=${toString cfg.metricRetentionDays}d"
              "--web.enable-remote-write-receiver"
              "--web.console.libraries=/usr/share/prometheus/console_libraries"
              "--web.console.templates=/usr/share/prometheus/consoles"
            ];
            volumes = [
              "${prometheusDir}:/prometheus"
              "${prometheusConfig}:/etc/prometheus/prometheus.yml:ro"
            ];
            # Loopback only: the agent on this machine writes here.
            ports = ["127.0.0.1:${toString cfg.prometheusPort}:9090"];
            networks = [network];
          };

          loki = {
            image = cfg.containers.loki;
            user = uidgid;
            cmd = ["-config.file=/etc/loki/neo.yaml"];
            volumes = [
              "${lokiDir}:/loki"
              "${lokiConfig}:/etc/loki/neo.yaml:ro"
            ];
            ports = ["127.0.0.1:${toString cfg.lokiPort}:3100"];
            networks = [network];
          };
        };

        neo.core.dockerNetworks = [network];
        virtualisation.oci-containers.containers.swag.extraOptions = ["--network=${network}"];

        systemd.services.grafana-seed = lib.neo.mkSetupService {
          inherit pkgs;
          name = "grafana-seed";
          description = "Import Neo's Grafana dashboards and alert rules once";
          containers = ["grafana"];
          path = [pkgs.docker pkgs.jq];
          retryInterval = 15;
          script = seedScript;
        };

        # Grafana alerts → Hermes (hermes/webhook.nix).
        neo.services.hermes.webhook.routes = mkIf alertsOn {
          grafana =
            {inherit deliver;}
            // (
              if cfg.assistantInvestigates && deliver != "log"
              then {
                skills = ["neo-monitoring"];
                prompt = ''
                  A monitoring alert from Grafana changed state ({status}). Look into it on this homeserver
                  (logs, unit status, the Grafana/Loki/Prometheus APIs per the neo-monitoring skill) and reply with
                  a short message for the owner: what is wrong, the likely cause and what to do. Do not change anything.

                  {__raw__}
                '';
              }
              else {
                deliver_only = deliver != "log";
                prompt = ''
                  {title}

                  {message}
                '';
              }
            );
        };
      };
    };
}
