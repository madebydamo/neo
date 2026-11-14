# Monitoring agent: Grafana Alloy on the host network, root, read-only views of
# the journal, /proc, /sys, / (as /rootfs), Docker and SWAG's logs.
#
# It sends to Loki and Prometheus of the Monitoring server: over loopback when
# that runs here, else through <serverUrl>/ingest/… with the ingest token.
# Config: _alloy.nix. Labels and metric names are what monitoring/_grafana.nix
# expects; change both together.
#
# Logs: the journal minus dockerd's journald-driver copy of container output
# (each docker-<c>.service already has it on stdout), SWAG's JSON access log
# (neo-access.log, written while the agent runs), tinyauth audit and sshd
# sign-ins as events. Numbers: node exporter (+ systemd, + a textfile that maps
# units to Neo services), cAdvisor, blackbox checks of this server's apps.
{...}: {
  flake.modules.nixos.monitoring-agent = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.monitoring-agent;
      server = config.neo.services.monitoring;
      swag = config.neo.services.swag;
      tinyauth = config.neo.services.tinyauth;
      domain = swag.domain or null;
      remote = cfg.serverUrl != null && cfg.serverUrl != "";
      base = removeSuffix "/" (cfg.serverUrl or "");
      tokenSet = cfg.token != null && cfg.token != "";
      hostName =
        if cfg.name != null && cfg.name != ""
        then cfg.name
        else config.neo.core.hostname;
      swagLogs = "${swag.appdata}/log/nginx";
      swagLogsOn = swag.enabled;

      proxied = lib.neo.getProxiedServices config;

      # Host name → service, for the `service` label of access logs.
      vhosts =
        concatLists (mapAttrsToList (name: svc:
          optional (domain != null && domain != "") {
            host = "${svc.subdomain}.${domain}";
            service = name;
          }
          ++ map (d: {
            host = d;
            service = name;
          }) (svc.customDomains or []))
        proxied)
        ++ map (d: {
          host = d;
          service = "proxy";
        }) (attrNames (swag.proxyPass or {}));

      # Apps reachable from the Internet, checked at their public address.
      appTargets = optionals (domain != null && domain != "") (mapAttrsToList (name: svc: {
          inherit name;
          address = "https://${svc.subdomain}.${domain}/";
        })
        (filterAttrs (_: svc: elem "web" (svc.ingress or [])) proxied));
      extraTargets =
        map (url: {
          name = url;
          address = url;
        })
        cfg.extraUptimeTargets;
      uptimeTargets = optionals cfg.uptimeChecks (appTargets ++ extraTargets);

      # neo_service_unit{service, name, kind}: which units make up which app.
      unitName = u:
        if hasInfix "." u
        then u
        else "${u}.service";
      esc = replaceStrings ["\\" "\"" "\n"] ["\\\\" "\\\"" "\\n"];
      serviceUnits = concatLists (mapAttrsToList (service: svc:
        map (u: {
          inherit service;
          name = unitName u;
          kind =
            if hasPrefix "docker-" u
            then "container"
            else "unit";
        }) (svc.systemdUnits or []))
      (filterAttrs (_: svc: (svc.enabled or false) && (svc.systemdUnits or []) != []) config.neo.services));
      textfile = pkgs.writeTextDir "neo.prom" (''
          # HELP neo_service_unit Systemd units that belong to a Neo service.
          # TYPE neo_service_unit gauge
        ''
        + concatMapStrings (u: ''
          neo_service_unit{service="${esc u.service}",name="${esc u.name}",kind="${u.kind}"} 1
        '')
        serviceUnits);

      # Checked at build time: a syntax error (e.g. from an app's strings)
      # would otherwise stop the whole agent. Argument errors still only show
      # when Alloy starts.
      alloyConfig = pkgs.runCommand "config.alloy" {nativeBuildInputs = [pkgs.grafana-alloy];} ''
        alloy validate ${alloyConfigRaw}
        cp ${alloyConfigRaw} $out
      '';
      # Scrape credentials (neo.monitoring.apps.*.metrics.*.token) are secret
      # placeholders: only the rendered env file of the container holds them.
      authTargets = filter (t: t.auth != "none") config.neo.monitoring.scrapeTargets;
      scrapeEnv = t: let
        ident = s: toUpper (replaceStrings ["-" "."] ["_" "_"] s);
      in "NEO_SCRAPE_${ident t.service}_${ident t.scrape}";
      alloyText = import ./_alloy.nix {
        inherit lib hostName vhosts uptimeTargets swagLogsOn scrapeEnv;
        appTargets = config.neo.monitoring.scrapeTargets;
        tokenSet = remote && tokenSet;
        lokiUrl =
          if remote
          then "${base}/ingest/loki/api/v1/push"
          else "http://127.0.0.1:${toString server.lokiPort}/loki/api/v1/push";
        prometheusUrl =
          if remote
          then "${base}/ingest/prometheus/api/v1/write"
          else "http://127.0.0.1:${toString server.prometheusPort}/api/v1/write";
      };
      alloyConfigRaw = pkgs.writeText "config.alloy" alloyText;

      # App dashboards (neo.monitoring.dashboards) go to the server's
      # apps/<source>/ directory: copied when Monitoring runs here, else PUT to
      # <serverUrl>/ingest/dashboards/<source>/<uid>.json (monitoring/swag.nix).
      # Files this source published before and no longer has are deleted.
      source = let
        s = toLower (replaceStrings [" " "_" "."] ["-" "-" "-"] hostName);
      in
        if builtins.match "[a-z0-9][a-z0-9-]*" s != null
        then s
        else "host-${substring 0 8 (builtins.hashString "sha256" hostName)}";
      dashboardDir = pkgs.runCommand "neo-app-dashboards" {} (''
          mkdir $out
        ''
        + concatStrings (mapAttrsToList (uid: d: ''
            cp ${pkgs.writeText "${uid}.json" (builtins.toJSON d)} $out/${uid}.json
          '')
          config.neo.monitoring.dashboards));
      tokenFile = config.neo.core.secrets.templates."monitoring-agent-token".path;
      publishOnce =
        if remote
        then ''
          token=$(cat ${tokenFile})
          state=${cfg.appdata}/published-dashboards
          touch "$state"
          for f in ${dashboardDir}/*.json; do
            [ -e "$f" ] || continue
            curl -fsS -o /dev/null -X PUT --data-binary @"$f" -H "Authorization: Bearer $token" \
              ${base}/ingest/dashboards/${source}/"$(basename "$f")"
          done
          while read -r old; do
            [ -n "$old" ] && [ ! -e ${dashboardDir}/"$old" ] || continue
            curl -sS -o /dev/null -X DELETE -H "Authorization: Bearer $token" ${base}/ingest/dashboards/${source}/"$old" || true
          done < "$state"
          ls ${dashboardDir} > "$state"
        ''
        else ''
          dest=${server.appdata}/apps/${source}
          install -d -o ${toString config.neo.core.uid} -g ${toString config.neo.core.gid} ${server.appdata}/apps "$dest"
          for f in "$dest"/*.json; do
            [ -e "$f" ] && [ ! -e ${dashboardDir}/"$(basename "$f")" ] && rm -f "$f"
          done
          for f in ${dashboardDir}/*.json; do
            [ -e "$f" ] || continue
            cmp -s "$f" "$dest/$(basename "$f")" || install -m 0644 -o ${toString config.neo.core.uid} -g ${toString config.neo.core.gid} "$f" "$dest/"
          done
        '';

      accessLogConf = ''
        # Generated by neo (monitoring-agent): JSON access log for the agent.
        log_format neo_json escape=json '{"time":"$time_iso8601","host":"$host","method":"$request_method","uri":"$uri","status":"$status","bytes":$body_bytes_sent,"request_time":$request_time,"remote_addr":"$remote_addr","user_agent":"$http_user_agent"}';
        access_log /config/log/nginx/neo-access.log neo_json;
      '';
      accessLogPath = "${swag.appdata}/nginx/conf.d/neo-access-log.conf";
    in {
      config = mkMerge [
        (mkIf cfg.enabled {
          assertions = [
            {
              assertion = remote || server.enabled;
              message = "neo.services.monitoring-agent: set serverUrl to your Monitoring server, or turn on Monitoring (services.monitoring) on this server.";
            }
            {
              assertion = !remote || tokenSet;
              message = "neo.services.monitoring-agent.token: set the ingest token of your Monitoring server (its Monitoring settings, Ingest token).";
            }
            {
              assertion = !(lib.neo.secrets.hasPlaceholder alloyText) && all (t: t.token != "" && !(hasInfix t.token alloyText)) authTargets;
              message = "monitoring-agent: a scrape credential is empty or would be written into config.alloy (store). Credentials go through neo.monitoring.apps.<service>.metrics.<scrape>.token only.";
            }
            {
              assertion = !remote || hasPrefix "https://" base || hasPrefix "http://" base;
              message = "neo.services.monitoring-agent.serverUrl must start with https:// (e.g. https://grafana.example.com).";
            }
          ];

          systemd.services.docker-alloy.preStart = lib.neo.mkEnsureDirs config [cfg.appdata];

          virtualisation.oci-containers.containers.alloy = {
            image = cfg.containers.alloy;
            user = "0:0";
            cmd = [
              "run"
              "--server.http.listen-addr=127.0.0.1:${toString cfg.port}"
              "--storage.path=/var/lib/alloy/data"
              "--disable-reporting"
              "/etc/alloy/config.alloy"
            ];
            volumes =
              [
                "${cfg.appdata}:/var/lib/alloy/data"
                "${alloyConfig}:/etc/alloy/config.alloy:ro"
                "${textfile}:/etc/neo-textfile:ro"
                "/var/log/journal:/var/log/journal:ro"
                "/run/log/journal:/run/log/journal:ro"
                "/etc/machine-id:/etc/machine-id:ro"
                "/:/rootfs:ro,rslave"
                "/sys:/sys:ro"
                "/var/lib/docker:/var/lib/docker:ro"
                "/dev/disk:/dev/disk:ro"
                "/var/run/docker.sock:/var/run/docker.sock:ro"
                # cAdvisor reads containers through Docker's containerd.
                "/run/docker/containerd/containerd.sock:/run/containerd/containerd.sock:ro"
                "/run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro"
              ]
              ++ optional swagLogsOn "${swagLogs}:/var/log/swag:ro";
            environment.TZ = config.neo.core.timeZone;
            networks = ["host"];
            extraOptions = ["--pid=host" "--cgroupns=host"];
          };

          neo.core.secrets.templates."monitoring-agent-token" = mkIf (remote && tokenSet) {
            content = cfg.token;
            mode = "0400";
          };

          # Long-running: publishes now (a new dashboard set restarts it) and
          # every hour, so the server heals after a reset or a missed upload.
          systemd.services.monitoring-agent-publish = {
            description = "Send this server's app dashboards to Monitoring";
            wantedBy = ["multi-user.target"];
            after = ["network-online.target"] ++ optional (!remote) "docker-grafana.service";
            wants = ["network-online.target"];
            path = [pkgs.coreutils pkgs.curl pkgs.diffutils];
            serviceConfig = {
              Type = "simple";
              Restart = "always";
              RestartSec = 60;
            };
            script = let
              once = pkgs.writeShellScript "monitoring-agent-publish-once" ''
                set -euo pipefail
                ${publishOnce}
              '';
            in ''
              while true; do
                if ${once}; then
                  echo "published $(ls ${dashboardDir} | wc -l) dashboard(s) as ${source}"
                  sleep 3600
                else
                  echo "publishing failed, retrying in 5 minutes" >&2
                  sleep 300
                fi
              done
            '';
          };

          neo.core.secrets.containerEnv.alloy = let
            env =
              optionalAttrs (remote && tokenSet) {
                NEO_INGEST_TOKEN = cfg.token;
              }
              // listToAttrs (map (t: nameValuePair (scrapeEnv t) t.token) authTargets);
          in
            mkIf (env != {}) env;
        })

        # Sign-in events: tinyauth's audit stream, as JSON the agent parses.
        # Guarded here: containers.tinyauth.* would create the container even
        # with tinyauth off.
        (mkIf (cfg.enabled && tinyauth.enabled) {
          virtualisation.oci-containers.containers.tinyauth.environment = {
            TINYAUTH_LOG_JSON = "true";
            TINYAUTH_LOG_STREAMS_AUDIT_ENABLED = "true";
          };
        })

        # SWAG keeps conf.d across restarts: write or remove the access log conf.
        (mkIf swag.enabled {
          systemd.services.docker-swag.preStart = mkAfter (
            if cfg.enabled
            then
              lib.neo.mkActivationScriptForFile config {
                filePath = accessLogPath;
                content = accessLogConf;
              }
            else "rm -f ${accessLogPath}"
          );
        })
      ];
    };
}
