# Monitoring server: Grafana + Prometheus + Loki on their own Docker network
# (`app-monitoring`, shared with SWAG only).
#
# Agents (monitoring-agent) write logs to Loki's push API and numbers to
# Prometheus' remote-write receiver: the one on this machine over loopback
# ports, others through SWAG at /ingest/… with the ingest token (swag.nix).
# Provided (read-only, updated by Neo): datasources, the Hermes contact point,
# Neo's dashboards and alert rules (store) and app dashboards the agents send
# (apps/). Everything people or Hermes create is theirs. Everyone tinyauth
# lets through signs in via the auth proxy header as admin; Hermes and
# grafana-seed use `docker exec grafana curl -H 'Remote-User: …' http://127.0.0.1:3000`.
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
      # container may reach them. SWAG joins it because the service has a site.
      network = lib.neo.docker.serviceNetwork "monitoring";

      alertsOn = cfg.assistantAlerts && hermes.enabled;
      hermesSecretPath = "/etc/grafana/neo/hermes-webhook.secret";
      telegramTokenPath = "/etc/grafana/neo/telegram-bot.token";
      # Hermes delivers to Telegram when a bot is set up, else only logs.
      deliver =
        if lib.neo.isSet hermes.telegramBotToken && hermes.telegramAllowedUserId != []
        then "telegram"
        else "log";
      # Critical alerts go to Grafana's own Telegram contact point as well
      # (contactPoints below): needs the same bot the assistant delivers with.
      criticalOn = alertsOn && cfg.criticalAlertsDirect && deliver == "telegram";
      criticalReceiver = "Assistant and Telegram";

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

      # Provided dashboards and alert rules are file-provisioned: read-only in
      # Grafana (people can "Save as" a copy), replaced whenever the files
      # change. Neo's own come from the store (folders Neo, ISO 27001); app
      # dashboards from apps/<source>/, written by the monitoring agents
      # (monitoring-agent/default.nix, via swag.nix for other machines).
      # Everything else (dashboards and rules people or Hermes make, the
      # notification policy) belongs to the users; grafana-seed only sets the
      # policy and the home dashboard while they are still Grafana's defaults.
      appsDir = "${cfg.appdata}/apps";

      # App alert rules (neo.monitoring.alertGroups of every machine): one JSON
      # file per source in alerts/, written by the agents (here directly, other
      # machines via swag.nix). Grafana reads file-provisioned alert rules only
      # when it starts, and keeps rules a file no longer has, so
      # grafana-app-alerts puts them in through the provisioning API instead:
      # one rule group per app in the Apps folder, provenance "api" (read-only
      # in Grafana). Rules it made that no file has any more are deleted; rules
      # people make in that folder are left alone. When two machines send the
      # same app, the newer file wins (the rules query every server anyway).
      alertsDir = "${cfg.appdata}/alerts";
      appsFolder = "neo-apps";
      alertSyncScript = getExe (pkgs.writeShellApplication {
        name = "grafana-app-alerts-sync";
        runtimeInputs = [pkgs.docker pkgs.jq pkgs.coreutils pkgs.gnused];
        runtimeEnv = {
          NEO_ALERTS_DIR = alertsDir;
          NEO_APPS_FOLDER = appsFolder;
          NEO_GRAFANA_API = "http://127.0.0.1:3000";
        };
        text = builtins.readFile ./alert-sync.sh;
      });

      # What every monitoring agent sends about itself (monitoring-agent/default.nix):
      # agents/<source>.json, file_sd format, its host label and the addresses it
      # checks. The local agent probes those addresses too (central uptime
      # checks), and monitoring-expected-hosts turns the files into
      # neo_monitoring_expected_host for "Server stopped reporting".
      agentsDir = "${cfg.appdata}/agents";
      expectedHostsScript = getExe (pkgs.writeShellApplication {
        name = "monitoring-expected-hosts";
        runtimeInputs = [pkgs.jq pkgs.coreutils];
        runtimeEnv = {
          NEO_AGENTS_DIR = agentsDir;
          NEO_METRICS_OUT = "${config.neo.monitoring.textfileDir}/neo-monitoring-hosts.prom";
        };
        text = builtins.readFile ./expected-hosts.sh;
      });

      dashboardDirs = pkgs.runCommand "neo-grafana-dashboards" {} (''
          mkdir $out
        ''
        + concatStrings (mapAttrsToList (uid: _: ''
            mkdir $out/${uid}
          '')
          grafanaLib.folders)
        + concatStrings (mapAttrsToList (name: d: ''
            cp ${pkgs.writeText "${name}.json" (builtins.toJSON d.dashboard)} $out/${d.folder}/${name}.json
          '')
          grafanaLib.dashboards));

      dashboardProviders = yaml.generate "dashboards.yaml" {
        apiVersion = 1;
        providers =
          mapAttrsToList (uid: f: {
            name = uid;
            folder = f.title;
            folderUid = uid;
            type = "file";
            disableDeletion = false;
            allowUiUpdates = false;
            updateIntervalSeconds = 60;
            options.path = "/var/lib/neo-dashboards/${uid}";
          })
          grafanaLib.folders
          ++ [
            {
              name = "apps";
              folder = "Apps";
              folderUid = "neo-apps";
              type = "file";
              disableDeletion = false;
              allowUiUpdates = false;
              updateIntervalSeconds = 30;
              options.path = "/var/lib/neo-apps";
            }
          ];
      };

      # rule {folderUID, ruleGroup} → Grafana's file format (groups by folder).
      alertRules = let
        groups = groupBy (r: "${r.folderUID}/${r.ruleGroup}") grafanaLib.alertRules;
      in
        yaml.generate "rules.yaml" {
          apiVersion = 1;
          groups =
            mapAttrsToList (_: rules: let
              r0 = head rules;
            in {
              orgId = 1;
              name = r0.ruleGroup;
              folder = grafanaLib.folders.${r0.folderUID}.title;
              interval = "1m";
              rules = map (r: removeAttrs r ["folderUID" "ruleGroup" "orgID"]) rules;
            })
            groups;
        };

      policy = pkgs.writeText "policy.json" (builtins.toJSON {
        receiver = "Assistant";
        group_by = ["grafana_folder" "alertname" "host"];
        group_wait = "30s";
        group_interval = "5m";
        repeat_interval = "12h";
        # Critical alerts take the contact point that also writes to Telegram
        # itself, so one dead path does not swallow them.
        routes = optional criticalOn {
          receiver = criticalReceiver;
          object_matchers = [["severity" "=" "critical"]];
        };
      });

      seedScript = let
        api = "http://127.0.0.1:3000";
      in
        ''
          # Inside the container, as the Grafana user "neo" (auth proxy header;
          # only SWAG and this host reach Grafana).
          g() { docker exec grafana curl -sS -o /dev/null -w '%{http_code}' -H 'Remote-User: neo' -H 'Content-Type: application/json' "$@"; }
          get() { docker exec grafana curl -sS -H 'Remote-User: neo' "$@"; }
          [ "$(g ${api}/api/health)" = 200 ]

          # Alert rule files name their folder by title, and on the first start
          # Grafana provisions them before the dashboard folders (fixed uids)
          # exist: the rules land in a second folder of the same title. Remove
          # such a copy when it holds nothing but Neo's provisioned rules; after a
          # restart the rules find Neo's folder by its title.
          folders=$(get ${api}/api/folders)
          rules=$(get ${api}/api/v1/provisioning/alert-rules)
          moved=0
          ${concatStrings (mapAttrsToList (uid: f: ''
              for dup in $(jq -r --arg t ${escapeShellArg f.title} '.[] | select(.title == $t and .uid != "${uid}") | .uid' <<<"$folders"); do
                dashboards=$(get "${api}/api/search?type=dash-db&folderUIDs=$dup" | jq length)
                other=$(jq --arg u "$dup" '[.[] | select(.folderUID == $u and .provenance != "file")] | length' <<<"$rules")
                if [ "$dashboards" != 0 ] || [ "$other" != 0 ]; then
                  echo "folder $dup (${f.title}) is a copy of ${uid} but has content of its own; left alone" >&2
                  continue
                fi
                code=$(g -X DELETE "${api}/api/folders/$dup?forceDeleteRules=true")
                case "$code" in 2*) echo "removed duplicate folder $dup (${f.title})"; moved=1 ;; *) echo "removing folder $dup failed: HTTP $code" >&2; exit 1 ;; esac
              done
            '')
            grafanaLib.folders)}
          if [ "$moved" = 1 ]; then
            # The rules come back from their file into Neo's folder.
            systemctl restart --no-block docker-grafana.service
            exit 1
          fi
          # Open the overview on the home page unless someone picked another one.
          home=$(get ${api}/api/org/preferences | jq -r '.homeDashboardUID // ""')
          if [ -z "$home" ]; then
            [ "$(g -X PATCH -d '{"homeDashboardUID":"neo-overview"}' ${api}/api/org/preferences)" = 200 ]
            echo "home dashboard set"
          fi
        ''
        + optionalString alertsOn ''
          # Point the default policy at the assistant unless someone changed it.
          current=$(get ${api}/api/v1/provisioning/policies)
          root=$(jq -r .receiver <<<"$current")
          routes=$(jq '.routes // [] | length' <<<"$current")
          # Untouched default: "empty" (Grafana 12+) or "grafana-default-email".
          # A policy Neo seeded before the critical route existed (receiver
          # Assistant, no child routes) gets that route too; a policy with
          # routes of its own is left alone.
          if [ "$root" = empty ] || [ "$root" = grafana-default-email ] ||
            { ${lib.boolToString criticalOn} && [ "$root" = Assistant ] && [ "$routes" = 0 ]; }; then
            code=$(docker exec -i grafana curl -sS -o /dev/null -w '%{http_code}' -H 'Remote-User: neo' -H 'Content-Type: application/json' \
              -H 'X-Disable-Provenance: true' -X PUT --data-binary @- ${api}/api/v1/provisioning/policies < ${policy})
            case "$code" in 2*) echo "notification policy set" ;; *) echo "policy failed: HTTP $code" >&2; exit 1 ;; esac
          fi
        '';

      # Signed like Hermes' generic V1 webhook: hex HMAC-SHA256 of the body.
      # title/message are what Hermes forwards to Telegram (route prompt): the
      # alert name and one line per alert, no link (Telegram would show a
      # preview of Grafana's home page under every alert).
      alertTitle = ''{{ if eq .Status "firing" }}🔴 {{ .CommonLabels.alertname }}{{ else }}✅ Resolved: {{ .CommonLabels.alertname }}{{ end }}'';
      alertBody = ''{{ range .Alerts }}{{ if eq .Status "resolved" }}(resolved) {{ end }}{{ or .Annotations.summary .Labels.alertname }}{{ "\n" }}{{ end }}'';
      hermesReceiver = uid: {
        inherit uid;
        type = "webhook";
        disableResolveMessage = false;
        settings = {
          url = "http://host.docker.internal:${toString hermes.webhook.port}/webhooks/grafana";
          httpMethod = "POST";
          hmacConfig = {
            secret = "$__file{${hermesSecretPath}}";
            header = "X-Webhook-Signature";
          };
          title = alertTitle;
          message = alertBody;
        };
      };
      # Second path for critical alerts: Grafana writes to the Telegram bot
      # API itself, so a critical alert still arrives when the assistant's
      # webhook does not answer (it, or the machine, being the problem). Same
      # bot and same chat as the assistant uses, so there is nothing new to
      # set up. Plain text: a summary may contain < or &, which Telegram
      # rejects in HTML mode.
      telegramReceiver = {
        uid = "neo-telegram";
        type = "telegram";
        disableResolveMessage = false;
        settings = {
          bottoken = "$__file{${telegramTokenPath}}";
          chatid = toString (head hermes.telegramAllowedUserId);
          parse_mode = "None";
          message = "${alertTitle}\n${alertBody}";
        };
      };
      contactPoints = yaml.generate "contact-points.yaml" {
        apiVersion = 1;
        contactPoints =
          [
            {
              orgId = 1;
              name = "Assistant";
              receivers = [(hermesReceiver "neo-hermes")];
            }
          ]
          ++ optional criticalOn {
            orgId = 1;
            name = criticalReceiver;
            receivers = [(hermesReceiver "neo-hermes-critical") telegramReceiver];
          };
      };

      provisioning = pkgs.runCommand "neo-grafana-provisioning" {} ''
        mkdir -p $out/datasources $out/dashboards $out/alerting $out/plugins $out/notifiers
        cp ${datasources} $out/datasources/neo.yaml
        cp ${dashboardProviders} $out/dashboards/neo.yaml
        cp ${alertRules} $out/alerting/rules.yaml
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

        neo.core.containerDirs.grafana = [cfg.appdata grafanaDir appsDir alertsDir];
        neo.core.containerDirs.prometheus = [cfg.appdata prometheusDir];
        neo.core.containerDirs.loki = [cfg.appdata lokiDir];

        # Grafana reads the bot token from the file ($__file{…} in the contact
        # point), so it never enters the store or the provisioning YAML.
        neo.core.secrets.templates."grafana-telegram-token" = mkIf criticalOn {
          content = hermes.telegramBotToken;
          owner = "homeserver";
          group = "homeserver";
          mode = "0400";
          restartUnits = ["docker-grafana.service"];
        };

        virtualisation.oci-containers.containers = {
          grafana = {
            image = cfg.containers.grafana;
            user = uidgid;
            volumes =
              [
                "${grafanaDir}:/var/lib/grafana"
                "${provisioning}:/etc/grafana/provisioning:ro"
                "${dashboardDirs}:/var/lib/neo-dashboards:ro"
                "${appsDir}:/var/lib/neo-apps:ro"
              ]
              ++ optional alertsOn "${hermes.webhook.secretFile}:${hermesSecretPath}:ro"
              ++ optional criticalOn "${config.neo.core.secrets.templates."grafana-telegram-token".path}:${telegramTokenPath}:ro";
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
            # Alerts go to Hermes' webhook on the host.
            networks = [network] ++ optional alertsOn lib.neo.docker.hostNetwork;
            extraOptions = optional alertsOn "--add-host=host.docker.internal:${lib.neo.docker.hostGateway}";
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
          };
        };

        systemd.services.grafana-seed = lib.neo.mkSetupService {
          inherit pkgs;
          name = "grafana-seed";
          description = "Set Grafana's home dashboard and alert routing while they are defaults";
          containers = ["grafana"];
          path = [pkgs.docker pkgs.jq pkgs.systemd];
          retryInterval = 15;
          script = seedScript;
        };

        # App alert rules → Grafana (see alertSyncScript). Runs again whenever a
        # file in alerts/ changes, and every hour.
        systemd.services.grafana-app-alerts = {
          description = "Put the apps' alert rules into Grafana";
          wantedBy = ["multi-user.target"];
          after = ["docker-grafana.service"];
          requires = ["docker-grafana.service"];
          partOf = ["docker-grafana.service"];
          path = [pkgs.docker pkgs.jq pkgs.coreutils pkgs.gnused pkgs.inotify-tools];
          serviceConfig = {
            Type = "simple";
            Restart = "always";
            RestartSec = 30;
            RuntimeDirectory = "grafana-app-alerts";
          };
          script = ''
            while true; do
              if ${alertSyncScript}; then wait=3600; else wait=60; fi
              # A change in alerts/ (an agent sent rules) or the timeout.
              inotifywait -qq -t "$wait" -e close_write,moved_to,delete ${alertsDir} || true
              sleep 2
            done
          '';
        };

        # Machines that must keep reporting (see expectedHostsScript): again
        # whenever a file in agents/ changes, and every hour.
        systemd.services.monitoring-expected-hosts = {
          description = "List the machines Monitoring expects to hear from";
          wantedBy = ["multi-user.target"];
          after = ["local-fs.target" "systemd-tmpfiles-setup.service"];
          path = [pkgs.coreutils pkgs.inotify-tools];
          serviceConfig = {
            Type = "simple";
            Restart = "always";
            RestartSec = 30;
          };
          script = ''
            ${lib.neo.mkEnsureDirs config [cfg.appdata agentsDir]}
            while true; do
              if ${expectedHostsScript}; then wait=3600; else wait=60; fi
              inotifywait -qq -t "$wait" -e close_write,moved_to,delete ${agentsDir} || true
              sleep 2
            done
          '';
        };

        # Grafana alerts → Hermes (hermes/webhook.nix).
        neo.services.hermes.webhook.routes = mkIf alertsOn {
          grafana =
            {inherit deliver;}
            // (
              if cfg.assistantInvestigates && deliver != "log"
              then {
                skills = ["neo-monitoring"];
                # Webhook turns get only web tools by default; looking into an
                # alert needs journalctl, systemctl, docker and curl.
                toolsets = ["terminal"];
                prompt = ''
                  Grafana alert ({status}). Find out what is broken on this homeserver: check the unit, target or
                  container named in the labels, its recent logs and whether it already recovered (commands and APIs
                  in the neo-monitoring skill). Read only: do not restart or change anything.

                  Your reply is a Telegram message to the owner and must be at most two short lines, no headings,
                  no lists, no Markdown: what is broken (app and host), then the cause in a few words if you found
                  one. Add a fix only when it is one obvious step. If it already recovered, say so in one line.
                  If the status is resolved, reply only "✅ <alert name> resolved".

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
