# Hermes skill for monitoring (also loaded for Grafana alerts it investigates).
{...}: {
  flake.modules.nixos.monitoring-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.monitoring;
  in {
    config.neo.services.monitoring.skill.conf = lib.neo.mkServiceSkill {
      service = "monitoring";
      inherit config;
      description = "Central logs, metrics, uptime checks and alerts (Grafana, Prometheus, Loki)";
      tags = ["neo" "monitoring" "grafana" "loki" "prometheus" "logs" "alerts"];
      body = ''
        ## When to Use
        Questions about uptime, why an app or server was down, log searches across servers, login events,
        web errors, and Grafana alerts (an alert may arrive as a webhook event for you to look into).

        ## Architecture notes
        - Containers `grafana`, `prometheus`, `loki`; data under `${cfg.appdata}`.
        - Every server runs `monitoring-agent` (Grafana Alloy). It sends:
          - journald (Loki `{job="journal", host, unit, level}`), SWAG access logs (`{job="swag", service, status_class}`),
            sign-ins from tinyauth and sshd (`{event="login"|"logout", result, provider}`, username/ip as structured metadata);
          - host numbers `node_*`, containers `container_*` (label `name`), uptime checks `probe_success{target}`,
            `neo_service_unit{service,name,kind}` (which units belong to which app), `neo_auth_events_total`,
            `neo_http_requests_total{service,status_class}`. Every series has `host`.
        - The agent here writes over loopback (Loki 127.0.0.1:${toString cfg.lokiPort}, Prometheus 127.0.0.1:${toString cfg.prometheusPort});
          other servers go through SWAG `/ingest/loki/api/v1/push` and `/ingest/prometheus/api/v1/write` with `Authorization: Bearer <ingestToken>`.
        - Containers sit on their own Docker network `app-monitoring` (with SWAG only); no other app's container can reach them. Grafana also joins `neo-host` for alerts to Hermes. The agent scrapes app metrics on each app's own network.
        - Two kinds of dashboards and alert rules:
          - Provided (read-only, Grafana refuses to save them; Neo replaces them on updates): folders Neo and ISO 27001
            from this server's configuration, and folder Apps with dashboards the monitoring agents send for the apps on
            their machine (`neo.monitoring.apps.<service>.dashboards`). Neo's alert rules are provided too.
          - User: everything people or you create. Neo never touches these.
          To change a provided dashboard, save a copy under a new uid ("Save as", or the API below) and change that.
          Datasources (uids `prometheus`, `loki`) and the contact point `Assistant` are provided as well.
        - App metrics: apps declare endpoints in `neo.monitoring.apps.<service>.metrics`; the agent on the machine running
          the app scrapes them, labels `service`, `endpoint`, `host`, `job="app/<service>"`.

        ## Querying (read-only, on this server)
        ```bash
        # PromQL
        curl -s 'http://127.0.0.1:${toString cfg.prometheusPort}/api/v1/query' --data-urlencode 'query=probe_success == 0'
        curl -s 'http://127.0.0.1:${toString cfg.prometheusPort}/api/v1/query' --data-urlencode 'query=node_systemd_unit_state{state="failed"} == 1'
        # LogQL (last hour; start/end in ns or RFC3339)
        curl -s -G 'http://127.0.0.1:${toString cfg.lokiPort}/loki/api/v1/query_range' \
          --data-urlencode 'query={host="nixos", unit="docker-immich.service"} |~ "(?i)error"' \
          --data-urlencode "start=$(date -d '-1 hour' +%s)000000000" --data-urlencode 'limit=100'
        curl -s 'http://127.0.0.1:${toString cfg.lokiPort}/loki/api/v1/labels'
        ```
        For units on this server `journalctl -u <unit>` is quicker; use Loki for other servers and history.

        ## Creating and changing dashboards and alert rules
        Use the Grafana HTTP API from inside the container, signed in as yourself through the auth proxy header
        (only this host and SWAG reach Grafana; you are an admin):
        ```bash
        g() { docker exec -i grafana curl -sS -H 'Remote-User: hermes' -H 'Content-Type: application/json' "$@"; }
        g http://127.0.0.1:3000/api/search?type=dash-db                      # list dashboards (uid, title, folderUid)
        g http://127.0.0.1:3000/api/dashboards/uid/neo-overview | jq .dashboard  # read one
        # create or update: {"dashboard": {...model, "uid": "my-uid", "id": null}, "folderUid": "neo", "overwrite": true, "message": "why"}
        g -X POST http://127.0.0.1:3000/api/dashboards/db -d @- < dashboard.json
        g http://127.0.0.1:3000/api/folders                                    # provided: neo, neo-iso27001, neo-apps
        g http://127.0.0.1:3000/api/v1/provisioning/alert-rules                # alert rules (same API to POST/PUT/DELETE)
        ```
        - Datasource refs: `{"type":"prometheus","uid":"prometheus"}` and `{"type":"loki","uid":"loki"}`. Copy panel shapes
          from a seeded dashboard; use the labels above, keep usernames/IPs out of `by (…)` on Prometheus.
        - Alert rules you create via the API: send `X-Disable-Provenance: true`, so people can still edit them in the UI.
          Route them to the `Assistant` contact point (the default policy) to reach the owner.
        - Check every query returns data (`/api/v1/query` above) before saving. Saving over a provided dashboard
          (folders Neo, ISO 27001, Apps) fails with "Cannot save provisioned dashboard": copy it to a new uid in another
          folder (create one with `POST /api/folders`) and say so.
        - Grafana keeps a version history: `GET /api/dashboards/uid/<uid>/versions`, restore via the UI.

        ## Alerts
        Grafana sends firing and resolved alerts to the Hermes webhook route `grafana`. The payload has `status`,
        `alerts[].labels` (alertname, host, target, name, service, mountpoint) and `alerts[].annotations.summary`.
        When asked to look into one: check the target or unit, its recent logs and whether it recovered, then answer
        in a few sentences. Do not restart or change anything unless the owner asks.
        Title and message of alerts are rendered by Grafana (contact point templates); the summary comes from each rule's `summary` annotation.

        ## Credentials
        - `services.monitoring.ingestToken`: Bearer token other servers' agents use (`services.monitoring-agent.token` there).
        - Grafana has no password: sign-in comes from the homeserver login (tinyauth `Remote-User`).

        ## Verification
        - `systemctl is-active docker-grafana docker-prometheus docker-loki docker-alloy`
        - `curl -s http://127.0.0.1:${toString cfg.lokiPort}/ready` → `ready`
        - Grafana: `docker exec swag curl -s http://grafana:3000/api/health`
      '';
    };
  };
}
