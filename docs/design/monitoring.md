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
| sshd `Accepted …` / `Failed …` / `Invalid user …` | same, `provider=ssh`; `attempt=scan` on a failure with a user name that does not exist (`Invalid user …`, `Failed … for invalid user …`) when sshd accepts keys only | same |
| SWAG `log/nginx/neo-access.log` (JSON, `conf.d/neo-access-log.conf` written while the agent runs) | `job=swag`, `service` (host → Neo service, else `other`), `status_class`; method, path, status, ip, vhost as structured metadata | `neo_http_requests_total`, `neo_http_request_duration_seconds` (without WebSockets: a 101 logs how long the connection stayed open) |
| node exporter (+ systemd, + textfile) | | `node_*`, `node_systemd_unit_state`, `neo_service_unit{service,name,kind}`, `neo_backup_*{destination}`, `neo_monitoring_expected_host{expected_host}` |
| cAdvisor | | `container_*` (label `name`), filtered to CPU, memory, network |
| blackbox | | `probe_*{app,target,app_host,probe_from}` |

**Textfile folder.** `neo.monitoring.textfileDir` (`/var/lib/neo/textfile`, tmpfiles `d` 0775 root:homeserver) is mounted read-only into Alloy as the node exporter's textfile folder. `docker-alloy.preStart` copies `neo.prom` (the `neo_service_unit` map, from the store) into it; other units write their own `<name>.prom` (write `x.prom.tmp`, then `mv`). It sits outside appdata on purpose: it describes this machine, and a restore must not bring back old values. Writers today:

- `backup.sh` → `neo-backup.prom`, after each destination: `neo_backup_last_run_timestamp_seconds`, `neo_backup_last_success_timestamp_seconds` and `neo_backup_last_run_failed` (1 or 0), all with label `destination`. The times live in `/var/lib/neo-backup/status-<destination>` between runs; the file lists only the destinations that are enabled now. A destination that never succeeded has no success time, only `last_run_failed 1`.
- `monitoring-expected-hosts` (Monitoring server only) → `neo-monitoring-hosts.prom` (see below).

Every series and stream carries `host` (`services.monitoring-agent.name`, default the host name). Labels stay low-cardinality: anything per user or per address is structured metadata, never a label. Log-derived counters come from Alloy's `stage.metrics` and reach Prometheus through `prometheus.exporter.self`.

`neo_service_unit` is generated from every enabled service's `systemdUnits`, so dashboards and alerts group units by app without hardcoding names (`kind=container` for `docker-*`).

Uptime checks target `https://<subdomain>.<domain>/` of every app on the machine whose `ingress` includes `web`, plus `extraUptimeTargets`. They go out through public DNS, so on a rathole setup they take the same path as a visitor. 2xx, 3xx, 401 and 403 count as up (tinyauth answers 302). Behind the login that 302 comes from SWAG and tinyauth even when the app is down, so an app with `neo.monitoring.apps.<service>.uptimePath` (a health path it answers itself, listed in `auth.publicPaths`) is checked at that path, and only 2xx counts as up there. A crash loop also raises "Service keeps restarting" (more than 3 automatic restarts in 15 minutes, `node_systemd_service_restart_total`): such a unit is "activating" most of the time and "failed" only for moments, which "Service failed" alone misses (issue #31). Local checks carry `app_host` and `probe_from`, both this machine's host label.

**Apps that are not published to the Internet** are checked as well, but not at their public address: a request from the machine itself arrives on the PROXY-protocol listener, counts as ingress class `web` and gets the Neo 404 page (meilen, 26 Nov 2026: a company website answered 404 to its own server, and nothing checked it at all). They are checked at SWAG's plain TLS listener instead (`swag.localHttpsPort` on 127.0.0.1), with the app's host name as SNI and `Host` header, which makes the class `local`. Each such app needs its own blackbox module for those two settings (`<module>_self_<app>`), and the `target` label keeps the app's real address, not 127.0.0.1. An app that is not open to the home network either (NetBird or Tailscale only) is not checked: nothing on that machine may open it. These checks are not sent to the Monitoring server (`agents/<source>.json` keeps the public addresses only).

**Checks from the Monitoring server.** When its machine is down, nothing checks its apps from there. So every agent also sends a file about itself, `agents/<source>.json` (file_sd format: a first entry with its `app_host` label, then one entry per public app address it checks, with `app`, `module` and a unique `name`), with `monitoring-agent-publish`: copied into `appdata/monitoring/agents/` on the server, else PUT to `/ingest/agents/<source>.json` (same token and WebDAV rules as the alert files; an older server answers 404, which only skips this). The agent on the Monitoring server (with **Check app uptime** on) reads that folder with `discovery.file` and checks every address there with its own blackbox exporter: only `http(s)://` addresses and Neo's own modules, never its own machine's (it checks those already). These series have `host` and `probe_from` = the Monitoring server and `app_host` = the machine that runs the app, so "App does not answer" fires for the apps of a machine that is down, once per place it is checked from. The rule sets the alert's `host` to `app_host` (`label_replace`; probes without `app_host` keep theirs) and names the checker from `probe_from`: "swag on uster (https://swag.example.com/) has not answered for 5 minutes (checked from meilen)". With `host` = the checker, the alert list showed host=meilen for uster's apps, which read as "meilen is down" (24 Nov 2026). The notification policy groups by `host`, so the alerts of one app machine arrive together. `extraUptimeTargets` are not sent: they may be LAN addresses (a router) only that machine reaches. For other views from outside, list the apps of server A in `extraUptimeTargets` of server B.

## Dashboards and alerts: provided and user

Two kinds, never mixed:

| | Where it comes from | In Grafana |
|---|---|---|
| **Provided** | Neo's own dashboards (`monitoring/_grafana.nix`: folder **Neo**; `_iso27001.nix`: **ISO 27001**) and alert rules, from the server's store. App dashboards (`neo.monitoring.apps.*.dashboards`), from the agent of each machine running the app: folder **Apps** | File-provisioned, `allowUiUpdates = false`: Grafana refuses to save them ("Cannot save provisioned dashboard", alert rules: provenance `file`). Replaced whenever the files change, removed with them. People copy one with Save as |
| **User** | People in the UI, Hermes through the API (`docker exec grafana curl -H 'Remote-User: hermes' …`) | Ordinary objects; Neo never touches them |

Datasources and the `Assistant` contact point are provided too. The notification policy and the home dashboard belong to the users: `grafana-seed` only sets them (policy → Assistant, home → Overview) while they still are Grafana's defaults.

App alert rules (`neo.monitoring.apps.*.alerts`) are provided too, in folder **Apps**, but through Grafana's provisioning API instead of files (see below), with provenance `api`: equally read-only.

Alert rules (folder Neo): app does not answer (5 min), server stopped reporting, disk over 90 %, systemd unit failed, systemd unit keeps restarting (more than 3 in 15 min), many failed sign-ins (more than 10 in 15 min per server and method, ends below 5), more than 20 % server errors, certificate expires in less than 10 days, certificate does not fit the address, many errors in the log (more than 500 lines of priority err or worse on a server in an hour), a service floods the log (more than 6 000 lines from one unit in 15 minutes), a program crashed (systemd-coredump wrote a dump). ISO 27001: clock not synchronised, backup overdue (26 h since the last success), backup failed, logs dropped, no logs from a server.

**Errors and volume in the log.** Both read the journal in Loki, per server and per unit, and both end in `or on () vector(0)` where the normal state returns no series at all (see "A rule has no data"). The thresholds sit far above a quiet fleet (25/26 Nov 2026: at most 157 error lines an hour on the busiest of five servers, loudest single unit 2 900 lines in 15 minutes). The volume rule is per unit, not per server: a server total is the sum of all its apps and says nothing about what changed. What a server writes with priority err is not what `journalctl -p err` on that server shows: dockerd copies every container line into the journal a second time with `CONTAINER_NAME` set, and a container writing to stderr makes those copies priority err (uster, 26 Nov 2026: 47 000 such lines a day, all of them ordinary tinyauth and crowdsec output). The agent drops those copies, so Loki has the lines once, with the level the app itself logged.

**Certificate does not fit the address.** `probe_ssl_earliest_cert_expiry` only says when the certificate ends, not whether it is the right one. A certificate for the wrong name, or a chain that does not verify, makes the uptime probe fail and reads as "App does not answer". So the agent checks every https address a second time with verification off (job `uptime-tls`, every 5 minutes, only `probe_success` kept): that check working while the normal one fails means the server answers and only the certificate is wrong. Apps checked at SWAG's own listener (below) verify the name through SNI already and need no second check.

**Failed sign-ins.** sshd on a server with SSH keys (`neo.core.ssh.authorizedKeys`) accepts keys only, so an attempt with a user name that does not exist cannot succeed: Internet scanners try `ubuntu`, `admin`, `ethereum`, … all day (24 Nov 2026: 319 such lines on three servers, the alert fired 14 times). The agent labels those `attempt="scan"` (only when sshd is keys-only); "Many failed sign-ins" leaves them out (`attempt!="scan"`), the Sign-ins panels show them as result `scan` (`label_replace` of `attempt` into `result`), the Failed logins stats leave them out and the ISO 27001 table of failed sign-ins has them with Kind `scan`. Failures of existing users (`Failed publickey for alice`) and of tinyauth count as before. The rule uses `recover = 5`: `increase()` extrapolates, so 10 real events read 9.3 or 10.3 and the alert went Normal/Alerting every minute.

**Recovery threshold.** `recover` on a rule (`lib.neo.grafana.alertRule`, `neo.monitoring.apps.*.alerts.*`) is Grafana's recovery threshold (`unloadEvaluator` of the threshold expression): a firing instance ends only when the value is below it (an `above` rule) or above it (a `below` rule). Use it for counts over a window that hover around the threshold; `appAlerts` sets it on `login-failures`.

**A rule has no data.** A query that counts events returns no series at all while nothing happens, and the rule's state then jumps between NoData and Normal every evaluation: an app's "errors in the log" rule did that 47 times in 24 hours (25/26 Nov 2026), each flip an annotation in the state history. Such a rule ends in `or on () vector(0)` (works in PromQL and in LogQL), which keeps one series with value 0. That series has no `host` label, and 0 never reaches a threshold, so it only holds the state at Normal. Rules whose query returns a series as long as the thing exists (a gauge, a uptime probe) do not need it; `noData = "ok"` stays for the cases where silence really is fine.

**A rule never resolves because its data is gone.** A rule that compares a series with its own past (`max_over_time(x[1d]) unless x`) turns Normal when the gap outgrows the window, while the problem goes on (brugg, 22–23 Nov 2026: down for a day, alert cleared after 24 h). So:

- **Server stopped reporting** compares with a list: `neo_monitoring_expected_host{expected_host}` on the Monitoring server, written by `monitoring-expected-hosts` (`monitoring/expected-hosts.sh`; long-running, runs on every change in `agents/` via inotify and hourly) with one line per agent file. A machine that is down keeps its file, so the alert stays until it reports again. **To retire a machine**, delete `appdata/monitoring/agents/<source>.json` on the Monitoring server (after turning its agent off; a running agent sends it again within the hour). Machines whose agent does not send that file yet (an older Neo) still fall back to "seen in the last day".
- **Backup overdue** reads `neo_backup_last_success_timestamp_seconds` per server and destination, so a run that fails (e.g. "cannot log in … No route to host") no longer counts. Servers whose backup does not export it yet fall back to the age of `backup.timer`'s last start. **Backup failed** fires per destination while its last run failed (`neo_backup_last_run_failed`) and clears with the next good run. A run that stops before any destination (the snapshot fails) changes neither: Backup overdue catches it after 26 hours.

## App metrics and dashboards

`neo.monitoring.apps.<service>` (`nix/modules/core/monitoring.nix`) is set by any service or plugin next to its code, e.g. `nix/services/immich/monitoring.nix`. Entries count on a machine only while their service is enabled there.

- `metrics.<scrape>`: a container and port (found through Docker discovery on `network`, default the container's first network, normally `app-<service>`) or an address. The agent scrapes it with labels `service`, `scrape` (not `endpoint`: CrowdSec and NetBird use that label themselves), `host`, `job="app/<service>"`, and applies the endpoint's `relabel` rules (e.g. Immich's `immich_jobs_<task>_<status>_total` → `immich_jobs_total{task, status}`). `neo.monitoring.collecting` tells the app whether to switch its exporter on.
- Credentials: `auth = "bearer" | "basic"` with `token` (default `scrapeToken`). Each app's `scrapeToken` is `sha256("neo-metrics:<service>:" + key)`, where the key is `neo.core.secrets.generated.metrics`: generated once per machine (`/var/lib/neo/secrets-generated/metrics`, copied into `raw/generated/` at every activation, so it is `@NEO_SECRET:generated/metrics@` with both backends). One token per app, so a token leaked from one container opens no other app's metrics. The scraper always runs on the app's machine, so the key never has to leave it. The token reaches Alloy as `NEO_SCRAPE_<SERVICE>_<SCRAPE>` in its rendered env file (`sys.env` in the config); an assertion keeps it out of `config.alloy`.
- `hidePaths`: metrics on the public port (Gitea `/metrics`, …) get `location = <path> { return 404; }` in every server block of the app's SWAG conf, whether or not an agent runs.
- `dashboards.<id>`: panels from `lib.neo.grafana` (or a JSON model). Neo sets uid `app-<service>-<id>`, tags and a `host` variable (`label_values({service="<service>"}, host)`).
- `alerts.<id>`: alert rules (`title`, `expr` PromQL or LogQL, `above`/`below`, `for`, `severity`, `summary`, `noData`), rendered with `lib.neo.grafana.alertRule` into one rule group per app (`neo.monitoring.alertGroups`), uid `app-<service>-<id>`, labels `service` and `severity`. They reach the same notification policy as Neo's rules, so with `assistantAlerts` the assistant gets them.
- Freshness rules (`lib.neo.monitoring.appAlerts`, `jobs` and `data`): `job-<name>` / `data-<source>` fire when the last success is older than the limit for 15 minutes, with no data = OK. A restart, a deploy or a move to another server leaves no series until the app's next run, and with `noData = "alerting"` and `for = "0s"` each of those fired at once (meilen, 22 Nov 2026, four times in 35 minutes). Silence has its own rule, `job-<name>-missing` / `data-<source>-missing`: `absent_over_time(…[15m])` on every server with `for` = the limit but at least 6 hours. The window is short and the tolerance is in `for`, so one sample does not clear the alert for the whole window: with `absent_over_time(…[4d])` one run of the Kanban digest at 06:30 ended a 16.5 h alert and left the rule blind until four days later (26 Nov 2026). Use the same pair for an app's own freshness rules instead of `noData = "alerting"`.
- **"It ran" and "it worked" are two signals.** A job that ends half done (one case failed, one source unreachable) must not set `neo_app_job_last_success_timestamp_seconds`, or the alert says nothing; but then "did not succeed" also covers the job that no longer runs at all, and the two need different fixes. An app that reports `neo_app_job_last_run_timestamp_seconds` for every run gets a third rule, `job-<name>-not-run`, and `job-<name>` steps back while that one has the case (`unless on (host) (age of last run > limit)`), so one stopped job stays one alert. Apps that do not report it keep both cases in `job-<name>`: the `unless` side is empty and takes nothing away. The virtual tester showed the problem live — "no tester tested" fired for meilen from 25 Nov 2026 09:07 while the tester ran every morning, because one `error` case made each run `incomplete`.

**App alert rules.** `monitoring-agent-publish` sends one JSON file per machine with all its apps' rule groups: copied to `appdata/monitoring/alerts/<source>.json` on the server, else PUT to `/ingest/alerts/<source>.json` (always, an empty `{}` clears that machine's rules; an older server answers 404 and only alerts are skipped). Grafana reads alert rule files only when it starts and keeps rules a file no longer has, so the server's `grafana-app-alerts` unit (long-running; runs on every change of `alerts/` via inotify and hourly) puts them in through the provisioning API: creates folder Apps (`neo-apps`) if needed, deletes rules with provenance `api` and an `app-` uid that no file has any more, and PUTs each app's rule group when it changed (hash in `/run`) or is missing. Rules people create in the Apps folder are left alone. When two machines send the same app, the newer file wins; app rules query every server anyway.

**Definition and data may live on different machines.** The dashboard travels with the metrics: `monitoring-agent-publish` on the machine that runs the app sends its rendered dashboards to the server. On the server itself it copies them into `appdata/monitoring/apps/<source>/`; elsewhere it PUTs `https://<grafana>/ingest/dashboards/<source>/<uid>.json` with the ingest token, which SWAG stores there with WebDAV (`dav_methods PUT DELETE`, regex-restricted names, no reads). Dashboards that machine published before and no longer has are deleted. It runs on every change of the set and hourly, so a reset server fills up again. So the server needs neither the app nor the plugin. When several machines run the same app they send the same uid; Grafana shows one, and its `host` variable switches between them.

The generated Alloy config is checked with `alloy validate` at build time (syntax and references); a wrong component argument still only shows when Alloy starts, so the generator keeps every app input typed.

### ISO 27001

The ISO dashboards map to Annex A (2022) controls: A.8.15/A.8.16/A.8.17/A.5.28 (logging, monitoring, clock, evidence), A.5.15–A.5.18/A.8.2/A.8.5 (access, privileged access, authentication), A.8.8/A.8.9/A.8.13/A.8.32 (vulnerabilities, configuration, backup, change), A.8.6/A.8.14/A.5.30 (capacity, availability), A.8.20/A.5.7/A.5.24–A.5.26 (network, threats, incidents). They are technical evidence only; the ISMS (scope, risk treatment, Statement of Applicability, policies, reviews) is not something a dashboard provides. For audits, raise `logRetentionDays` to the period the organisation's policy names (often 90–365 days).

Extra data they use: sudo lines as `{event="privileged", provider="sudo"}` (username, target, command, reason as structured metadata; `unit="session.scope"` means a person, other units are services), timer last-trigger times (`node_systemd_timer_last_trigger_seconds`, never-run timers are left out), `node_timex_*`, and the agent's own delivery counters (`loki_write_*_entries_total`, `prometheus_remote_storage_samples_*`). Sign-in sessions are folded into one `session.scope` unit value to keep labels bounded.

With `assistantAlerts` and Hermes on the same machine, the provisioned contact point is a webhook to Hermes (`hermes/webhook.nix`):

- `neo.services.hermes.webhook.routes.grafana`, rendered into Hermes' `platforms.webhook.extra.routes`.
- The HMAC key is generated once at activation (`webhook.secretFile`), handed to Hermes as `WEBHOOK_SECRET` and to Grafana as a read-only file (`$__file{}` in the contact point's `hmacConfig`). Grafana signs the body with it as `X-Webhook-Signature`, which is Hermes' generic HMAC check.
- `deliver_only` sends the alert text straight to the Telegram home channel; title and message come from the contact point's templates (the alert name, then one line per alert from its `summary`; no link, which Telegram would expand into a preview). With `assistantInvestigates`, Hermes runs a turn with the `neo-monitoring` skill and the `terminal` toolset (webhook turns otherwise only get web tools) and sends its reply, at most two lines, instead.
- Hermes deep-merges its settings into the `config.yaml` on disk, so `hermes/webhook.nix` gives every route `skills`, `toolsets` and `deliver_only` explicitly; otherwise turning `assistantInvestigates` off would keep the skill, and Hermes would prepend the whole skill text to every plain alert.
- Summaries read query values through `{{ with $values.B }}…{{ else }}…{{ end }}`: a rule without data (`noData = "alerting"`, or the NoData state) has no `$values.B`, the template fails and Grafana sends it unrendered.
- Without a Telegram bot the route delivers to Hermes' log.

**A second path for critical alerts.** One contact point means one thing to break: a webhook to a Hermes that is down, or on a machine that is down, swallows every alert silently. With `criticalAlertsDirect` (on by default, needs the assistant's Telegram bot) Grafana gets a second contact point, **Assistant and Telegram**, that holds the same webhook *and* Grafana's own Telegram integration, and the seeded notification policy routes `severity = critical` to it (`object_matchers`). So the worst alerts arrive twice in the same chat: once in the assistant's words, once straight from Grafana, which needs nothing but the Telegram API. The bot token reaches Grafana as a read-only file (`$__file{}`, `neo.core.secrets.templates."grafana-telegram-token"`), never in the provisioning YAML; the message is plain text, because a summary may contain characters Telegram rejects in HTML mode. The route is part of the seeded policy, so a policy somebody has edited is still left alone.

The Hermes listener binds all interfaces on port 8644, which is not opened in the firewall: only the host and its Docker bridges reach it.

## Storage

Everything lives under `appdata/monitoring/{grafana,prometheus,loki,apps,alerts,agents}` and `appdata/monitoring-agent` (Alloy's WAL and positions), so snapshots, backup and Clear appdata cover it. Retention: `logRetentionDays` (Loki compactor) and `metricRetentionDays` (Prometheus TSDB).
