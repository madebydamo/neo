# App telemetry

How a Neo service or plugin makes its app understandable: logs that say what went wrong, a few standard numbers, a dashboard built from standard sections, and alert rules that ship with the app. Every app uses the same names, so `lib.neo.grafana.app` and `lib.neo.monitoring.appAlerts` (`nix/lib/app-telemetry.nix`) build its dashboard and alerts without per-app queries.

Order of work: **logs first** (errors become visible without code), then the **standard metrics** the app has (AI calls, sign-ins, features, jobs, data freshness), then app-specific metrics and panels.

## What Neo already collects (no app code)

| Source | Query |
|--------|-------|
| Container output | Loki `{job="journal", unit="docker-<container>.service"}` |
| Web requests through SWAG | `neo_http_requests_total{service, status_class}`, `neo_http_request_duration_seconds`; Loki `{job="swag", service="<svc>"}` with structured metadata `path`, `status`, `ip`, `method` |
| Container CPU/memory | `container_*{name="<container>"}` |
| Uptime, 5xx share, tinyauth sign-ins | Neo's own alerts (`App does not answer`, `App answers with errors`, `Many failed sign-ins`): do not repeat them per app |

## Logs

One JSON object per line on stdout:

```json
{"ts":"2026-11-16T08:52:22.599Z","level":"error","logger":"grader","msg":"grading failed","event":"grade_failed","exam":12,"exc":"Traceback …"}
```

- `level`: `debug|info|warning|error|critical`. Error panels and the `errors` alert read this field; anything that went wrong for a user or a job is `error` with `exc`.
- `event`: a stable name for things worth counting in LogQL (`login`, `job_done`, `llm_error`, …).
- Never swallow an exception: log it (`log.exception(...)` / `log.error(msg, {err})`) or re-raise. Background tasks (`asyncio.create_task`, threads, timers) need a done-callback or try/except that logs.
- No secrets, prompts or answers of users in logs (ids and sizes are fine). Default level `info`; `LOG_LEVEL` overrides.
- Third-party images that cannot log JSON: pass an `errorFilter`/`filter` LogQL pipeline that keeps their error lines (e.g. ``|~ `(?i)\b(error|fatal|exception)\b` ``).

## Standard metrics

Served on **`$NEO_METRICS_PORT` (9464)**, which the Nix module sets only while monitoring collects. The port stays on the service network (never proxied). `service` and `host` labels are added by the agent; do not add them.

| Metric | Labels | Meaning |
|--------|--------|---------|
| `neo_app_llm_requests_total` | `provider, model, feature, status` | AI calls; `status` = `ok\|error\|timeout\|rate_limited\|auth` (`auth` also for 402 out of credit) |
| `neo_app_llm_tokens_total` | `provider, model, feature, kind` | `kind` = `input` (**uncached** input), `cache_read`, `cache_write`, `output` |
| `neo_app_llm_cost_usd_total` | `provider, model, feature` | Cost as the provider reports it (OpenRouter `usage.cost`, Claude Code `total_cost_usd`) |
| `neo_app_llm_request_duration_seconds` | `provider, model, feature` | Histogram |
| `neo_app_logins_total` | `method, result` | Sign-ins the app itself checks; `result` = `success\|failure` |
| `neo_app_active_users` | `window` | Different users seen in `1h`, `24h`, `7d` (apps behind tinyauth: the `Remote-User` of requests) |
| `neo_app_feature_used_total` | `feature` | One per user-visible action (`export_pdf`, `ask`, `solve`); a fixed, small set of names |
| `neo_app_errors_total` | `component, kind` | Errors (the helpers count every error log line) |
| `neo_app_job_runs_total` | `job, status` | Background job runs, `success\|failure` |
| `neo_app_job_last_success_timestamp_seconds` | `job` | Unix time of the last success |
| `neo_app_job_duration_seconds` | `job` | Histogram |
| `neo_app_data_updated_timestamp_seconds` | `source` | Unix time the app last got fresh data from an outside source |
| `neo_app_build_info` | `version` | 1 |

`feature` names are code constants, never user input. App-specific metrics are welcome next to these: prefix them with the app (`tmr_gps_samples_total`), not `neo_app_`.

### Helpers to copy

Do not reimplement these; copy the file unchanged into the app:

- Python: `templates/plugin/telemetry/python/neo_telemetry.py` (needs `prometheus_client`): `setup_logging()`, `start_metrics()`, `build_info()`, `llm_call()` / `record_llm()` (usage of Anthropic, Claude Code, OpenAI, OpenRouter and Gemini → the four token kinds), `classify_error()`, `feature()`, `login()`, `seen_user()`, `job()`, `data_updated()`.
- Node: `templates/plugin/telemetry/node/neo-telemetry.mjs` (needs `prom-client`): the same in camelCase; `setupLogging()` also turns `console.*` into JSON lines.

Token rules the helpers implement: Anthropic `input_tokens` is already uncached; OpenAI/OpenRouter `prompt_tokens` includes `prompt_tokens_details.cached_tokens`, so uncached = prompt − cached; streaming calls record the usage of the final chunk (OpenAI: `stream_options: {include_usage: true}`; OpenRouter: `usage: {include: true}` for cost).

## Browser-only apps

Static sites and games have no server code: SWAG's access log already gives the `web` section (with referring sites). For in-app events, the page requests `GET /_e/<event>[/<detail>]` (`navigator.sendBeacon` or `fetch(url, {keepalive: true})`) and gets a 204. SWAG logs the request either way; the `beacon` section counts events by their first path segment.

Answer the 204 **in the app** when it has its own nginx or server (proxied through the vhost's `location /`, so tinyauth applies), with `access_log off` there since SWAG already logs it:

```nginx
location ^~ /_e/ { access_log off; add_header Cache-Control "no-store"; return 204; }
```

Only when SWAG serves the files itself, answer in the vhost, and keep the login in front: a bare `return` runs before tinyauth's check and would make the path public, so the 204 comes from `try_files`:

```nix
# swag.nix, inside the server block
location ^~ /_e/ {
  ${lib.neo.authBlock config cfg}
  try_files /dev/null =204;
}
```

Event names are fixed constants; details may be small numbers or ids (`/_e/solve/42s`), never personal data. Cap events per page view (e.g. 200) and send nothing for automated browsers (`navigator.webdriver`).

Apps without a metrics endpoint pass `metrics = false` to the `errors` section (no `neo_app_errors_total` panel).

## Nix

```nix
# modules/services/<svc>/monitoring.nix
neoPlugin.modules.myapp-monitoring = {config, lib, plugin, ...}: let
  name = plugin.name "myapp";
  cfg = config.neo.services.${name};
in {
  config = lib.mkIf cfg.enabled {
    virtualisation.oci-containers.containers.${name}.environment =
      lib.mkIf config.neo.monitoring.collecting {NEO_METRICS_PORT = "9464";};

    neo.monitoring.apps.${name} = {
      metrics.main = {container = name; port = 9464;};
      dashboards.overview = {
        title = "MyApp${plugin.label}";
        description = "What people do in MyApp, its AI use and its errors.";
        time = "now-7d";
        panels = with lib.neo.grafana.app;
          # App-specific headline panels first (lib.neo.grafana), then:
          features {service = name;}
          ++ llm {service = name;}
          ++ logins {service = name;}
          ++ jobs {service = name;}
          ++ web {service = name;}
          ++ errors {service = name; containers = [name];};
      };
      alerts = lib.neo.monitoring.appAlerts {
        service = name;
        label = "MyApp${plugin.label}";
        containers = [name];
        llm = true;               # AI provider failing / key rejected
        logins = true;            # many failed sign-ins (own login only)
        jobs.nightly = 26 * 3600; # last success older than this
        data.feed = 2 * 3600;     # data source older than this
      } // {
        # App-specific rules (neo.monitoring.apps.<svc>.alerts.<id>).
      };
    };
  };
};
```

Sections (`lib.neo.grafana.app.*`, each a list of panels starting with a row): `web`, `beacon`, `logins`, `features`, `llm`, `jobs`, `errors {containers; filter?}`. Only add the sections whose metrics the app exports; an empty panel is noise.

`appAlerts` (`service`, `label`, `containers`, `errorFilter`, `errorThreshold` = 10 error lines in 15 min or null, `llm`, `logins`, `jobs`, `data`) returns an attrset for `alerts`. Severity `critical` for things that stop the app from working for users (AI provider failing, key rejected), `warning` otherwise. Every summary says what is wrong in plain words.

A Python app's image needs `prometheus_client` (Node: `prom-client`) in its dependencies; an image the plugin builds with Nix adds it there.
