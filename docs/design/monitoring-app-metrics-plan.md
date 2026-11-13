# Plan: app metrics endpoints and dashboards

Status: plan, not implemented. Builds on [monitoring.md](monitoring.md) (`neo.monitoring.apps`, `lib.neo.grafana`, the Immich example in `nix/services/immich/monitoring.nix`).

Goal: every app with a usable Prometheus endpoint gets **one file**, `nix/services/<app>/monitoring.nix`, that turns its exporter on, protects it, declares the scrape and ships a dashboard. Everything app-independent (secrets, auth in Alloy, label names, hiding paths from the internet) lives in shared code built once in phase 0, so each app file stays short.

Research was done against the image versions we pin (Nov 2026). Items marked *verify* were not confirmed against a live scrape.

## Phase 0: shared groundwork (one agent, before any app)

### 0.1 One generated scrape key per machine

Apps that need a credential for their metrics get one derived from a **single machine-generated key**. The operator never sees or sets it.

- New in `nix/modules/core/secrets.nix`: `neo.core.secrets.generated.<name> = { bytes ? 32; }`.
  - At activation, `/var/lib/neo/secrets-generated/<name>` (root 0400, hex) is created if missing.
  - Inside the `neo-secrets` activation, after the sops decrypt (which `rm -rf`s `raw/`), it is copied to `raw/generated/<name>`.
  - So `@NEO_SECRET:generated/<name>@` works in `containerEnv`, `templates`, `sha256`, `jsonString` and the leak guard, like any setting secret. It works with both backends.
  - Losing the file (a reinstall) just yields a new key. Every consumer is rendered from it at activation and restarted by `restartUnits`, so nothing breaks.
- `nix/modules/core/monitoring.nix` declares `neo.core.secrets.generated.metrics = {}` and gives each app a read-only **`neo.monitoring.apps.<svc>.scrapeToken`**:
  `lib.neo.secrets.sha256 "neo-metrics:<svc>:${mkPlaceholder "generated/metrics"}"`
  - That is one shared secret, but a **different 64-hex token per app**. A token leaked from one container (through env or `/proc`) opens no other app's metrics.
  - Hex suits every mechanism found: Gitea bearer, Karakeep bearer, Vikunja and Dawarich basic auth, the SearXNG password, the Nextcloud serverinfo token.
- Why not a setting:
  - It never leaves the machine. The agent that scrapes an app always runs on the app's machine.
  - An operator-set secret would only add a field to fill in and a value in the config repo.
  - The Hermes webhook secret (`nix/services/hermes/webhook.nix`) could move to the same mechanism later.

### 0.2 Auth and headers in the scrape helper

Extend `metricType` in `nix/modules/core/monitoring.nix`:

```nix
auth = mkOption { type = enum [ "none" "bearer" "basic" ]; default = "none"; };
username = mkOption { type = str; default = "neo-metrics"; };   # basic only
token = mkOption { type = str; default = <app>.scrapeToken; };   # override e.g. with a Meilisearch key
headers = mkOption { type = attrsOf str; default = {}; };       # non-secret, e.g. X-Forwarded-Proto
```

In `nix/services/monitoring-agent/` (`_alloy.nix` + `default.nix`):

- For every target with `auth != "none"`, put `NEO_SCRAPE_<SVC>_<EP> = t.token` into `neo.core.secrets.containerEnv.alloy`.
- Render it as `bearer_token = sys.env("…")` or `basic_auth { username = "…" password = sys.env("…") }`.
- Render `headers` as `http_headers = { "X-Forwarded-Proto" = ["https"] }`.
- `scrapeTargets` must then carry `token`. It is a placeholder, so it may only reach `containerEnv`, never the store-built `config.alloy`. An assertion should catch a token string inside `alloyConfigRaw`.
- One Alloy component may use only one auth kind: an endpoint is either bearer or basic.

### 0.3 Rename our `endpoint` label

CrowdSec (`cs_lapi_request_duration_seconds{endpoint}`) and NetBird (`management_http_*{endpoint}`) carry their own `endpoint` label, which our target label turns into `exported_endpoint`.

- Rename ours to **`scrape`**, both in `_alloy.nix` and in the docs (LIB.md, PLUGIN-DEVELOPMENT.md, design doc, skills).
- Check the Immich dashboard for uses: it currently uses none.

### 0.4 Keep metrics paths off the internet

Gitea `/metrics`, Vikunja `/api/v1/metrics`, Karakeep `/api/metrics`, Collabora `/cool/getMetrics` and Grafana `/metrics` are all served on the app's public port. Some sit on tinyauth public paths (`^/api/`), so they are reachable from the internet.

- Add `neo.monitoring.apps.<svc>.hidePaths` (listOf str, exact paths or `^~` prefixes).
- The reverse-proxy location generator (`nix/lib/reverseProxy/authorization.nix`, the `extraLocations` join near line 164) emits `location = <p> { return 404; }` for the service's vhost.
- Find out how that function learns the service name, or pass `hidePaths` through `mkSwagServer`.
- This applies whether or not the agent runs: a path should never be public just because nobody scrapes it.

### 0.5 Smaller helper additions

- `metrics.<ep>.params` (attrsOf (listOf str)) → `params` in `prometheus.scrape`. Phase 2's multi-target DB exporters need it.
- A shared relabel snippet `lib.neo.grafana.dropOtelScope` (`labeldrop otel_scope_name|otel_scope_version` + drop `target_info`). Immich, NetBird and anything OTel-based use it.
- A `lib.neo.monitoring.nameToLabels regex labels name` helper, moved out of `immich/monitoring.nix` because Karakeep and others want the same thing.
- Docs: LIB.md (new fields, `scrapeToken`, `hidePaths`), PLUGIN-DEVELOPMENT.md, the plugin template `monitoring.nix`, the `neo-plugin` and `neo-core` skills, and a line in AGENTS.md "Monitoring".

**Phase 0 done when:** `just check` passes and the VM scrapes a test endpoint with bearer and with basic auth. The Immich dashboard still works after the rename. `curl https://<app>/<hidden path>` returns 404.

## Phase 1: apps with a built-in endpoint (one subagent per app)

Each agent writes `nix/services/<app>/monitoring.nix` and nothing else. If the app's `default.nix` truly has to change, it says why. Env vars, `containerEnv`, `cmd` additions and setup services are all set from `monitoring.nix` (modules merge), gated with `lib.mkIf config.neo.monitoring.collecting` like Immich. The exception is `hidePaths`, which is always on.

| App | Endpoint(s) | Turn on | Auth | hidePaths |
|---|---|---|---|---|
| **Authentik** | `authentik:9300`, `authentik-worker:9300` (any path) | on by default | none, internal only | – |
| **CrowdSec** (`apps.swag`, gated on `swag.crowdsec.enabled`) | `crowdsec:6060/metrics` | on by default. Pin `prometheus.level: full` in `config.yaml.local` (same activation helper as `acquis.d/neo-swag.yaml`, `swag/crowdsec.nix:30-40`) | none | – |
| **Gitea** | `gitea:<cfg.port>/metrics`, every 60s | `GITEA__metrics__ENABLED=true`; `containerEnv.gitea.GITEA__metrics__TOKEN = scrapeToken` | bearer | `/metrics` |
| **Vikunja** (native) | `127.0.0.1:<cfg.port>/api/v1/metrics` | `services.vikunja.settings.metrics.enabled = true`; `environmentFiles` from a secrets template with `VIKUNJA_METRICS_USERNAME=neo-metrics`, `VIKUNJA_METRICS_PASSWORD=<token>` | basic | `/api/v1/metrics` |
| **Docker registry** | `registry:5001/metrics` | on in the image; pin `REGISTRY_HTTP_DEBUG_ADDR=0.0.0.0:5001`, `…_DEBUG_PROMETHEUS_ENABLED=true`, `…_PATH=/metrics` | none. The debug port also serves pprof on `internal` (already the case today) | – |
| **NetBird server** | `netbird-server:9090/metrics` (management, signal, relay) | on by default; optionally `metricsPort: 9090` in the config template | none | – |
| **Dawarich** | `dawarich-sidekiq:9394/metrics` only | both containers: `PROMETHEUS_EXPORTER_ENABLED=true`, `METRICS_USERNAME=neo-metrics`, `SIDEKIQ_METRICS_URL=http://dawarich-sidekiq:9394/metrics`; `containerEnv` `METRICS_PASSWORD` | basic | – (`:3000/metrics` is not scraped: Rails host check + force_ssl. Sidekiq has all the job metrics) |
| **Karakeep** | `karakeep:3000/api/metrics`, `karakeep:9091/metrics` (workers), `karakeep-meilisearch:7700/metrics` | `WORKERS_HOST=0.0.0.0`, `WORKERS_PORT=9091`, `containerEnv.karakeep.PROMETHEUS_AUTH_TOKEN`; Meilisearch `MEILI_EXPERIMENTAL_ENABLE_METRICS=true` | bearer. Meilisearch: bearer with `token = cfg.meiliMasterKey` (Alloy runs as root on the host already, so a scoped key would add a setup service for little gain) | `/api/metrics` |
| **SearXNG** | `searxng:8080/metrics` (`vpn:8080` when `vpn.enabled`) | setup service (shape: syncthing's `mkSetupService`) that sets `general.open_metrics` in `${appdata}/searxng/settings.yml` from a secrets-template file, then restarts searxng once | basic (username ignored) | `/metrics` |
| **Syncthing** | `syncthing:8384/metrics` | always on | none while no GUI password is set (our case). **Don't** set `STGUIAPIKEY`: any API key is full REST admin, and SWAG's `/rest` skips tinyauth | `/metrics` |
| **Collabora** | `collabora:9980/cool/getMetrics` | `cmd` `--o:security.enable_metrics_unauthenticated=true`, `--o:admin_console.logging.metrics_fetch=false` | none (on purpose: an admin password would make the admin console a live login on the internet) | `/cool/getMetrics`, `/cool/adminws`, `^~ /browser/dist/admin/` |
| **Monitoring itself** (`apps.monitoring`) | `127.0.0.1:<prometheusPort>`, `127.0.0.1:<lokiPort>`, `grafana:3000` on network `monitoring` | on by default | none | Grafana `/metrics` |

### Dashboard content per app

All dashboards use `lib.neo.grafana` panels and the `appSelector`. Container CPU and memory (cAdvisor) and an errors-and-warnings log panel come at the bottom, as in Immich. Hand-built in every case: the community dashboards found are stale (Authentik 14837, CrowdSec 19010/19012) or use labels we don't have (NetBird upstream). Use them for query ideas only.

- **Authentik**
  - Flow executions per flow: `authentik_flows_plan_time_count` by `flow_slug`.
  - Stage and policy time.
  - Tasks: `authentik_tasks_total`, `_errors_total`, `_queued`, `_in_progress`; worker count; outposts connected.
  - Request rate and latency (`django_http_*`, `authentik_main_request_duration_seconds`).
  - There is no login counter, so add a Loki panel for login and login_failed events (*verify* the log format).
  - Drop the `obj_pk` label from `authentik_policies_engine_time_total_seconds`.
- **CrowdSec**
  - Active bans by reason and origin (`cs_active_decisions`), alerts, scenarios fired (`cs_bucket_overflowed_total`).
  - SWAG bouncer checks ok/ko, log lines parsed ok/ko per source, community-list last pull.
  - Drop `cs_node_.*`.
  - Link from the ISO "Network" dashboard.
- **Gitea**
  - Users (active/inactive), repos, orgs, issues open/closed, PRs, releases, mirrors, webhooks, hook tasks; Go memory, goroutines and CPU.
  - Leave `ENABLED_ISSUE_BY_*` off.
- **Vikunja**: users, projects, tasks, teams, files, active users and link shares; DB pool (`go_sql_*`).
- **Registry**
  - Pushes (`manifest` PUT) and pulls (`manifest` GET) over time, request latency per handler, 5xx.
  - Storage action time, cache hit ratio (`registry_storage_cache_hits_total / _requests_total`).
- **NetBird**
  - Connected peers (`management_grpc_connected_streams`), logins and blocked logins, sync rate and latency.
  - Signal active peers and forward failures; relay peers and bytes.
  - IdP errors.
  - Drop the OTel scope labels and `management_updatechannel_*` (*verify* the histogram suffixes on a live scrape).
- **Dawarich**
  - Jobs executed, failed and enqueued by worker; queue length and latency; retries and dead jobs; busy workers.
  - Archive operations and verification failures.
- **Karakeep**
  - Users, bookmarks, asset size, bookmarks created by source.
  - Queue jobs by queue and status, worker stats, crawler status codes, crawl latency.
  - tRPC errors, HTTP latency.
  - Meilisearch: documents per index, DB size, indexing, task queue latency.
- **SearXNG**
  - Requests and results per engine (rate of the counters), reliability per engine, response time per engine.
  - It has no HTTP metrics, so add request numbers from the SWAG logs (`neo_http_requests_total{service="searxng"}`).
- **Syncthing**
  - Folder state timeline (`syncthing_model_folder_state` as a state timeline with value mappings 0–9).
  - Files and bytes per folder, needed (out-of-sync) items, conflicts.
  - Connected devices, sent and received bytes per device, scan and pull time.
- **Collabora**
  - People editing and documents open, load time and WOPI up/download time, bytes to and from clients, memory.
  - Crashes and OOM kills.
  - Drop `doc_.*`: it carries file names.
- **Monitoring health**
  - Samples and log lines ingested per host; Loki discarded lines by reason; remote-write rejections.
  - Prometheus head series and TSDB size; rule evaluation failures; failed notifications to Hermes.
  - Agent drops (`loki_write_dropped_entries_total`, `prometheus_remote_storage_samples_failed_total`, already scraped).
  - Fits ISO A.8.15/A.8.16 as "the logging itself works".

### Alert rules

Apps may ship provided rules next to their dashboards. That needs a small extension: `neo.monitoring.apps.<svc>.alertRules` using the existing rule helper from `monitoring/_grafana.nix`, published the same way as dashboards. Rule candidates:

- CrowdSec: community list not pulled for 24h.
- Dawarich and Karakeep: failed jobs rising.
- Syncthing: a folder in error state for 15 min.
- Monitoring: Loki discarding lines, or notifications failing.
- Authentik: no connected workers.

This is optional and could be a separate phase.

## Phase 2: no built-in endpoint

Do these after phase 1, using the same pattern.

- **Our own jobs, as textfile metrics.**
  - `/etc/neo-textfile` is a read-only store dir today. Make it `/var/lib/neo/textfile`: tmpfiles `d`, 0775 root:homeserver, copy `neo.prom` in during `docker-alloy.preStart`, mount it read-only.
  - Writers write `x.prom.$$` and then `mv`.
  - backup (per destination, written after `prune_snapshots`): `neo_backup_last_success_timestamp_seconds{destination,target,format}`, `_last_status`, `_duration_seconds`, `_bytes_added`, `_unreadable_files`.
  - docker-updater and system-updater (in `finalize()`): last run, last success, duration, images updated or changed, failures, reboot pending.
  - Feeds the ISO backup dashboard with real success times instead of timer age.
- **Databases.**
  - One shared exporter container per engine, so Docker DNS works. Databases sit on their service's network (`app-<service>`, [network-isolation.md](network-isolation.md)), so the exporter joins the network of every service whose database it probes, like SWAG (add it to the hubs of the `docker-network-isolation` check), or each service gets its own exporter sidecar:
    - `prometheuscommunity/postgres-exporter` (`/probe?target=…&auth_module=…`)
    - `prom/mysqld-exporter` (`/probe`)
    - `oliver006/redis_exporter` (`/scrape?target=…`)
  - Credentials come from one secrets template per exporter.
  - Apps declare `neo.monitoring.apps.<svc>.databases.<name> = { engine; container; database; user; password; }`.
  - Prefer this over Alloy's built-in exporters: those need a fixed address, Alloy has no Docker DNS, and an empty discovery list is an evaluation error that could stop the whole agent.
  - Needs `params` from 0.5.
  - Inventory: Postgres for authentik, docmost, activepieces, immich, paperless and dawarich; MariaDB for nextcloud (needs a monitoring user with PROCESS); Redis/Valkey for docmost, activepieces, immich, paperless, dawarich, nextcloud and searxng.
  - One generic "Databases" dashboard (connections, size, transactions, slow queries, Redis memory and hits).
- **Sidecar exporters.**
  - **Nextcloud:** `ghcr.io/xperimental/nextcloud-exporter` on :9205. `occ config:app:set serverinfo token --value <scrapeToken>` in the existing occ setup service; the exporter reads it with `NEXTCLOUD_AUTH_TOKEN=@/file`. No admin account needed.
  - **Pi-hole v6:** `ekofr/pihole-exporter` (≥ 1.3.0) on :9617 with `PIHOLE_PASSWORD` from `webPassword`. *Verify* that its v6 support is complete and that it reuses its API session (Pi-hole limits sessions).
  - **Paperless:** `hansmi/prometheus-paperless-exporter` needs an API token for a user with view rights. We have no admin user or token today, so this needs a token-creating setup service. Last priority.
- **Not planned:** Vaultwarden (upstream metrics PR not merged), and the apps with nothing to expose (see the evaluation in the conversation of 2026-11-13). Their container, log and uptime data already covers them.

## How to run it

1. **Phase 0:** one agent, on `tmp`, small commits:
   - secrets `generated`
   - helper auth, headers and params
   - label rename
   - hidePaths
   - docs and skills
   Then `just launch` and smoke test.
2. **Phase 1:** one subagent per app in its own worktree (`isolation: "worktree"`). Each commits there and the lead cherry-picks onto `tmp` (see the subagent commit rule). Brief per agent:
   - the table row and dashboard bullets above;
   - read `nix/services/immich/monitoring.nix` and docs/LIB.md "Grafana";
   - only write `nix/services/<app>/monitoring.nix`;
   - `git add`, `just format && just check`.
   The VM run happens once after merging, not per agent: one VM, one port.
3. **Verify on the VM after the merge:**
   - every target `up == 1` (`up{job=~"app/.*"}`);
   - every dashboard's queries return data with 0 errors (same check script as for the ISO dashboards);
   - hidden paths return 404 from outside;
   - no secret placeholder in `/etc/alloy/config.alloy`.
   Apps not enabled in the dev `settings.toml` get enabled temporarily: watch VM memory (Immich was OOM-killed once).
4. **Phase 2:** as its own round, textfile metrics first: they are small and help the ISO dashboards.

## Open points to check while implementing

- NetBird histogram suffixes, and whether the Authentik Rust histogram renders as a summary: curl once and adjust the queries.
- Which tinyauth/SWAG path rules already apply to Gitea (`auth = false` today); `hidePaths` must win over them.
- SearXNG with VPN: the scrape container becomes `vpn`. `monitoring.nix` picks it from `cfg.vpn.enabled`.
- Collabora's image is `latest`, so metric names may drift. The dashboard should tolerate missing series.
