# Monitoring: central logs, metrics, uptime and alerts

Two services:

| Service | Runs | Role |
|---------|------|------|
| `monitoring` | one machine | Grafana (dashboards, alerts), Prometheus (numbers), Loki (logs), containers on their own `monitoring` network |
| `monitoring-agent` | every machine, the central one too | Grafana Alloy: collects and pushes to the central server |

Beszel stays for small homeservers that only want a live view. Monitoring keeps history, every log line and alerts. They can run side by side, but nothing requires both.

## Data flow

```
journald ─┐
SWAG JSON access log ─┤                         ┌─ loopback 127.0.0.1:3100/9090   (agent on the central machine)
node/systemd/textfile ─┼─ Alloy (host network) ─┤
cAdvisor, blackbox ───┘                         └─ https://<grafana>/ingest/…      (other machines, Bearer token)
                                                         │
                                       SWAG: token check, no tinyauth
                                                         │
                                     loki:3100/loki/api/v1/push, prometheus:9090/api/v1/write
```

Agents push; the server never connects to them, so monitored machines need no open ports. Prometheus runs with `--web.enable-remote-write-receiver` and scrapes nothing itself.

## Ingest authentication

`services.monitoring.ingestToken` (secret, Generate) is checked by SWAG on `/ingest/loki/api/v1/push` and `/ingest/prometheus/api/v1/write`, the only paths that skip tinyauth. The check lives in a rendered snippet (`lib.neo.secrets.swagSnippet`), so the token never enters the store. Without a token those paths answer 403 and only the local agent can report. Loki and Prometheus themselves have no authentication, and Grafana trusts `Remote-User`. So the three sit on their own Docker network `app-monitoring` (like every service, [network-isolation.md](network-isolation.md)), which only SWAG joins besides them; other containers cannot reach them. The local agent uses loopback ports. The vhost's **Reachable from** (`ingress`) also applies to the ingest paths: a remote agent over the Internet needs `web`.

The agent reads the token from `services.monitoring-agent.token` through `neo.core.secrets.containerEnv` (`NEO_INGEST_TOKEN`, `sys.env` in the Alloy config).

## Grafana sign-in

Grafana uses its auth proxy: SWAG sets `Remote-User`/`Remote-Email`/`Remote-Name` from tinyauth, users are created on first visit as org Admin, the login form and the initial admin account are off. When the vhost's login is turned off, SWAG clears those headers, so nobody can sign in rather than everybody impersonating anyone.

Machine access (the seed unit, Hermes) runs `docker exec grafana curl -H 'Remote-User: <name>' http://127.0.0.1:3000/…`: whoever can use Docker on the host is root anyway.

## What the agent collects

| Source | Loki labels | Metrics |
|--------|-------------|---------|
| journald (minus dockerd's `CONTAINER_NAME` copies: oci-containers already logs each container on `docker-<c>.service`) | `job=journal`, `unit`, `level` | |
| tinyauth audit stream (`TINYAUTH_LOG_JSON`, `…_AUDIT_ENABLED` while the agent runs) | `event`, `result`, `provider`; `username`, `ip`, `reason` as structured metadata | `neo_auth_events_total` |
| sshd `Accepted …` / `Failed …` / `Invalid user …` | same, `provider=ssh` | same |
| SWAG `log/nginx/neo-access.log` (JSON, `conf.d/neo-access-log.conf` written while the agent runs) | `job=swag`, `service` (host → Neo service, else `other`), `status_class`; method, path, status, ip, vhost as structured metadata | `neo_http_requests_total`, `neo_http_request_duration_seconds` |
| node exporter (+ systemd, + textfile) | | `node_*`, `node_systemd_unit_state`, `neo_service_unit{service,name,kind}` |
| cAdvisor | | `container_*` (label `name`), filtered to CPU, memory, network |
| blackbox | | `probe_*{app,target}` |

Every series and stream carries `host` (`services.monitoring-agent.name`, default the host name). Labels stay low-cardinality: anything per user or per address is structured metadata, never a label. Log-derived counters come from Alloy's `stage.metrics` and reach Prometheus through `prometheus.exporter.self`.

`neo_service_unit` is generated from every enabled service's `systemdUnits`, so dashboards and alerts group units by app without hardcoding names (`kind=container` for `docker-*`).

Uptime checks target `https://<subdomain>.<domain>/` of every app on the machine whose `ingress` includes `web`, plus `extraUptimeTargets`. They go out through public DNS, so on a rathole setup they take the same path as a visitor. 2xx, 3xx, 401 and 403 count as up (tinyauth answers 302). For a view from outside the machine's own network, list the apps of server A in `extraUptimeTargets` of server B.

## Dashboards and alerts: provided and user

Two kinds, never mixed:

| | Where it comes from | In Grafana |
|---|---|---|
| **Provided** | Neo's own dashboards (`monitoring/_grafana.nix`: folder **Neo**; `_iso27001.nix`: **ISO 27001**) and alert rules, from the server's store. App dashboards (`neo.monitoring.apps.*.dashboards`), from the agent of each machine running the app: folder **Apps** | File-provisioned, `allowUiUpdates = false`: Grafana refuses to save them ("Cannot save provisioned dashboard", alert rules: provenance `file`). Replaced whenever the files change, removed with them. People copy one with Save as |
| **User** | People in the UI, Hermes through the API (`docker exec grafana curl -H 'Remote-User: hermes' …`) | Ordinary objects; Neo never touches them |

Datasources and the `Assistant` contact point are provided too. The notification policy and the home dashboard belong to the users: `grafana-seed` only sets them (policy → Assistant, home → Overview) while they still are Grafana's defaults.

Alert rules (folder Neo): app does not answer (5 min), server stopped reporting, disk over 90 %, systemd unit failed, many failed sign-ins (10 in 15 min), more than 20 % server errors, certificate expires in less than 10 days. ISO 27001: clock not synchronised, backup overdue (26 h), logs dropped, no logs from a server.

## App metrics and dashboards

`neo.monitoring.apps.<service>` (`nix/modules/core/monitoring.nix`) is set by any service or plugin next to its code, e.g. `nix/services/immich/monitoring.nix`. Entries count on a machine only while their service is enabled there.

- `metrics.<endpoint>`: a container and port (found through Docker discovery on `network`, default the container's first network, normally `app-<service>`) or an address. The agent scrapes it with labels `service`, `endpoint`, `host`, `job="app/<service>"`, and applies the endpoint's `relabel` rules (e.g. Immich's `immich_jobs_<task>_<status>_total` → `immich_jobs_total{task, status}`). `neo.monitoring.collecting` tells the app whether to switch its exporter on.
- `dashboards.<id>`: panels from `lib.neo.grafana` (or a JSON model). Neo sets uid `app-<service>-<id>`, tags and a `host` variable (`label_values({service="<service>"}, host)`).

**Definition and data may live on different machines.** The dashboard travels with the metrics: `monitoring-agent-publish` on the machine that runs the app sends its rendered dashboards to the server. On the server itself it copies them into `appdata/monitoring/apps/<source>/`; elsewhere it PUTs `https://<grafana>/ingest/dashboards/<source>/<uid>.json` with the ingest token, which SWAG stores there with WebDAV (`dav_methods PUT DELETE`, regex-restricted names, no reads). Dashboards that machine published before and no longer has are deleted. It runs on every change of the set and hourly, so a reset server fills up again. So the server needs neither the app nor the plugin. When several machines run the same app they send the same uid; Grafana shows one, and its `host` variable switches between them.

The generated Alloy config is checked with `alloy validate` at build time (syntax and references); a wrong component argument still only shows when Alloy starts, so the generator keeps every app input typed.

### ISO 27001

The ISO dashboards map to Annex A (2022) controls: A.8.15/A.8.16/A.8.17/A.5.28 (logging, monitoring, clock, evidence), A.5.15–A.5.18/A.8.2/A.8.5 (access, privileged access, authentication), A.8.8/A.8.9/A.8.13/A.8.32 (vulnerabilities, configuration, backup, change), A.8.6/A.8.14/A.5.30 (capacity, availability), A.8.20/A.5.7/A.5.24–A.5.26 (network, threats, incidents). They are technical evidence only; the ISMS (scope, risk treatment, Statement of Applicability, policies, reviews) is not something a dashboard provides. For audits, raise `logRetentionDays` to the period the organisation's policy names (often 90–365 days).

Extra data they use: sudo lines as `{event="privileged", provider="sudo"}` (username, target, command, reason as structured metadata; `unit="session.scope"` means a person, other units are services), timer last-trigger times (`node_systemd_timer_last_trigger_seconds`, never-run timers are left out), `node_timex_*`, and the agent's own delivery counters (`loki_write_*_entries_total`, `prometheus_remote_storage_samples_*`). Sign-in sessions are folded into one `session.scope` unit value to keep labels bounded.

With `assistantAlerts` and Hermes on the same machine, the provisioned contact point is a webhook to Hermes (`hermes/webhook.nix`):

- `neo.services.hermes.webhook.routes.grafana`, rendered into Hermes' `platforms.webhook.extra.routes`.
- The HMAC key is generated once at activation (`webhook.secretFile`), handed to Hermes as `WEBHOOK_SECRET` and to Grafana as a read-only file (`$__file{}` in the contact point's `hmacConfig`). Grafana signs the body with it as `X-Webhook-Signature`, which is Hermes' generic HMAC check.
- `deliver_only` sends the alert text straight to the Telegram home channel; title and message come from the contact point's templates (one line per alert from its `summary`). With `assistantInvestigates`, Hermes runs a turn with the `neo-monitoring` skill first and sends its summary instead.
- Without a Telegram bot the route delivers to Hermes' log.

The Hermes listener binds all interfaces on port 8644, which is not opened in the firewall: only the host and its Docker bridges reach it.

## Storage

Everything lives under `appdata/monitoring/{grafana,prometheus,loki,apps}` and `appdata/monitoring-agent` (Alloy's WAL and positions), so snapshots, backup and Clear appdata cover it. Retention: `logRetentionDays` (Loki compactor) and `metricRetentionDays` (Prometheus TSDB).
