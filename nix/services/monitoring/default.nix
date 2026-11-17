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
      # Hermes delivers to Telegram when a bot is set up, else only logs.
      deliver =
        if lib.neo.isSet hermes.telegramBotToken && hermes.telegramAllowedUserId != []
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
      alertSyncScript = let
        api = "http://127.0.0.1:3000";
      in
        pkgs.writeShellScript "grafana-app-alerts-sync" ''
          set -euo pipefail
          state=/run/grafana-app-alerts
          # req METHOD PATH [BODY]: prints the body, then the HTTP code on the last line.
          req() {
            if [ $# -ge 3 ]; then
              printf '%s' "$3" | docker exec -i grafana curl -sS -w '\n%{http_code}' -H 'Remote-User: neo' \
                -H 'Content-Type: application/json' -X "$1" --data-binary @- "${api}$2"
            else
              docker exec grafana curl -sS -w '\n%{http_code}' -H 'Remote-User: neo' -X "$1" "${api}$2"
            fi
          }
          code() { tail -n1 <<<"$1"; }
          body() { sed '$d' <<<"$1"; }

          [ "$(code "$(req GET /api/health)")" = 200 ] || { echo "Grafana is not up yet" >&2; exit 1; }

          r=$(req GET /api/folders/${appsFolder})
          if [ "$(code "$r")" = 404 ]; then
            r=$(req POST /api/folders '{"uid":"${appsFolder}","title":"Apps"}')
            case "$(code "$r")" in 2*|409|412) ;; *) echo "creating the Apps folder failed: $(body "$r")" >&2; exit 1 ;; esac
          fi

          # Valid files only, oldest first, so the newest wins for an app.
          desired='{}'
          while read -r f; do
            [ -n "$f" ] || continue
            if jq -e 'type == "object"' "$f" >/dev/null 2>&1; then
              desired=$(jq -c --slurpfile add "$f" '. + $add[0]' <<<"$desired")
            else
              echo "$(basename "$f"): not a JSON object, ignored" >&2
            fi
          done < <(ls -1tr ${alertsDir}/*.json 2>/dev/null || true)

          r=$(req GET /api/v1/provisioning/alert-rules)
          [ "$(code "$r")" = 200 ] || { echo "reading alert rules failed: $(body "$r")" >&2; exit 1; }
          ours=$(body "$r" | jq -c '[.[] | select(.folderUID == "${appsFolder}" and .provenance == "api" and (.uid | startswith("app-")))]')

          # Rules no app declares any more.
          for uid in $(jq -r --argjson d "$desired" '($d | [.[].rules[].uid]) as $want | .[] | select([.uid] | inside($want) | not) | .uid' <<<"$ours"); do
            r=$(req DELETE "/api/v1/provisioning/alert-rules/$uid")
            case "$(code "$r")" in 2*|404) echo "removed alert rule $uid" ;; *) echo "removing $uid failed: $(body "$r")" >&2; exit 1 ;; esac
          done

          mkdir -p "$state"
          for group in $(jq -r 'keys[]' <<<"$desired"); do
            grp=$(jq -c --arg g "$group" '.[$g]' <<<"$desired")
            sum=$(sha256sum <<<"$grp" | cut -d' ' -f1)
            # Unchanged and every rule still there (one may have been deleted through the API).
            have=$(jq -c --arg g "$group" '[.[] | select(.ruleGroup == $g) | .uid] | sort' <<<"$ours")
            want=$(jq -c '[.rules[].uid] | sort' <<<"$grp")
            if [ "$have" = "$want" ] && [ "$(cat "$state/$group" 2>/dev/null)" = "$sum" ]; then
              continue
            fi
            r=$(req PUT "/api/v1/provisioning/folder/${appsFolder}/rule-groups/$group" "$grp")
            case "$(code "$r")" in
              2*) echo "$sum" >"$state/$group"; echo "alert rules of $group: $(jq '.rules | length' <<<"$grp")" ;;
              *) echo "alert rules of $group failed: $(body "$r")" >&2; exit 1 ;;
            esac
          done
        '';

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
          root=$(get ${api}/api/v1/provisioning/policies | jq -r .receiver)
          # Untouched default: "empty" (Grafana 12+) or "grafana-default-email".
          if [ "$root" = empty ] || [ "$root" = grafana-default-email ]; then
            code=$(docker exec -i grafana curl -sS -o /dev/null -w '%{http_code}' -H 'Remote-User: neo' -H 'Content-Type: application/json' \
              -H 'X-Disable-Provenance: true' -X PUT --data-binary @- ${api}/api/v1/provisioning/policies < ${policy})
            case "$code" in 2*) echo "notification policy set" ;; *) echo "policy failed: HTTP $code" >&2; exit 1 ;; esac
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

        systemd.services.docker-grafana.preStart = lib.neo.mkEnsureDirs config [cfg.appdata grafanaDir appsDir alertsDir];
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
                "${dashboardDirs}:/var/lib/neo-dashboards:ro"
                "${appsDir}:/var/lib/neo-apps:ro"
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
