# Standard app telemetry: lib.neo.grafana.app (dashboard sections) and
# lib.neo.monitoring.appAlerts (alert rules) over the metric names every app
# exports the same way (docs/APP-TELEMETRY.md):
#
#   neo_app_llm_requests_total{provider, model, feature, status}     ok|error|timeout|rate_limited|auth
#   neo_app_llm_tokens_total{provider, model, feature, kind}         input (uncached) | cache_read | cache_write | output
#   neo_app_llm_cost_usd_total{provider, model, feature}
#   neo_app_llm_request_duration_seconds{provider, model, feature}   histogram
#   neo_app_logins_total{method, result}                             success|failure
#   neo_app_active_users{window}                                     gauge, window = 1h|24h|7d
#   neo_app_feature_used_total{feature}
#   neo_app_errors_total{component, kind}
#   neo_app_job_runs_total{job, status}                              success|failure
#   neo_app_job_last_success_timestamp_seconds{job}
#   neo_app_data_updated_timestamp_seconds{source}
#
# Logs: one JSON object per line with `level` (debug|info|warning|error|critical)
# and `msg`; errors carry `exc` (traceback). The error panels and the `errors`
# alert read that `level`.
#
#   dashboards.overview.panels = with lib.neo.grafana.app;
#     web {service = name;} ++ llm {service = name;} ++ errors {service = name; containers = [name];};
#   alerts = lib.neo.monitoring.appAlerts {service = name; label = "MyApp"; containers = [name]; llm = true;};
{lib, ...}: let
  g = (import ./grafana.nix {inherit lib;}).libExtensions.grafana.neo.grafana;
  inherit (g) pq lq lqLegend row stat timeseries bars bargauge logs table appSelector alertSelector loki thresholdsGreenRed;

  # Journal units of the app: its containers plus host units (timers' services, …).
  unitRe = containers: units:
    lib.concatStringsSep "|" (map (c: "docker-${c}[.]service") containers
      ++ map (u:
        builtins.replaceStrings ["."] ["[.]"] (
          if lib.hasInfix "." u
          then u
          else "${u}.service"
        ))
      units);
  # Log lines of the app's containers and units whose JSON `level` is error or worse.
  errorLines = {
    containers,
    units ? [],
    filter ? null,
    # Dashboards pick servers with $host; alert rules see every server.
    hostSel ? ", host=~\"$host\"",
  }:
    ''{job="journal", unit=~"${unitRe containers units}"${hostSel}} ''
    + (
      if filter != null
      then filter
      else ''| json lvl="level" | lvl=~"(?i)(error|critical|fatal)"''
    );

  red = g.thresholdsGreenRed [
    {
      color = "red";
      value = 1;
    }
  ];
in {
  libExtensions.appTelemetry.neo = {
    grafana.app = {
      inherit errorLines;

      # Web traffic through SWAG (every proxied app has it, no app code).
      web = {
        service,
        title ? "Web",
      }: let
        sel = appSelector service;
      in [
        (row title null)
        (stat {
          title = "Requests";
          w = 4;
          h = 4;
          description = "Web requests in the chosen time range.";
          targets = [(pq "sum(increase(neo_http_requests_total{${sel}}[$__range]))" "")];
          decimals = 0;
          thresholds = thresholdsGreenRed [];
          options.colorMode = "none";
        })
        (stat {
          title = "Visitors";
          w = 4;
          h = 4;
          description = "Different addresses that opened the app in the chosen time range.";
          datasource = loki;
          targets = [(lq ''count(sum by (ip) (count_over_time({job="swag", service="${service}", host=~"$host"} [$__range])))'' // {queryType = "instant";})];
          options.colorMode = "none";
        })
        (stat {
          title = "Server errors";
          w = 4;
          h = 4;
          description = "Answers with a 5xx status in the chosen time range.";
          targets = [(pq ''sum(increase(neo_http_requests_total{${sel}, status_class="5xx"}[$__range])) or vector(0)'' "")];
          decimals = 0;
          thresholds = red;
        })
        (timeseries {
          title = "Requests by status";
          w = 12;
          h = 8;
          unit = "reqps";
          targets = [(pq "sum by (status_class) (rate(neo_http_requests_total{${sel}}[$__rate_interval]))" "{{status_class}}")];
        })
        (timeseries {
          title = "Response time (95th percentile)";
          w = 12;
          h = 8;
          unit = "s";
          targets = [(pq "histogram_quantile(0.95, sum by (le) (rate(neo_http_request_duration_seconds_bucket{${sel}}[$__rate_interval])))" "p95")];
        })
        (bargauge {
          title = "Top pages";
          w = 12;
          h = 8;
          datasource = loki;
          targets = [(lqLegend ''topk(15, sum by (path) (count_over_time({job="swag", service="${service}", host=~"$host"} | status=~"2..|3.." != "/_e/" [$__range])))'' "{{path}}")];
        })
        (bargauge {
          title = "Referring sites";
          w = 12;
          h = 8;
          description = "Other sites whose links brought visitors here.";
          datasource = loki;
          targets = [(lqLegend ''topk(15, sum by (ref) (count_over_time({job="swag", service="${service}", host=~"$host"} | referer=~"https?://.+" | label_format ref=`{{ regexReplaceAll "^https?://([^/:]+).*" .referer "''${1}" }}` | label_format ext=`{{ if eq .ref .vhost }}0{{ else }}1{{ end }}` | ext="1" [$__range])))'' "{{ref}}")];
        })
        (bargauge {
          title = "Missing pages (404)";
          w = 12;
          h = 8;
          datasource = loki;
          targets = [(lqLegend ''topk(15, sum by (path) (count_over_time({job="swag", service="${service}", host=~"$host"} | status="404" [$__range])))'' "{{path}}")];
        })
      ];

      # Browser events sent as `GET /_e/<event>[/<detail>]` (204 from SWAG or
      # the app): counted from the access log, no metrics endpoint needed.
      beacon = {
        service,
        title ? "Events",
      }: [
        (row title null)
        (bargauge {
          title = "Events";
          w = 12;
          h = 10;
          description = "Browser events in the chosen time range.";
          datasource = loki;
          targets = [(lqLegend ''sum by (event) (count_over_time({job="swag", service="${service}", host=~"$host"} | path=~"/_e/.+" | label_format event=`{{ regexReplaceAll "^/_e/([^/]+).*" .path "''${1}" }}` [$__range]))'' "{{event}}")];
        })
        (bars {
          title = "Events over time";
          w = 12;
          h = 10;
          datasource = loki;
          targets = [(lqLegend ''sum by (event) (count_over_time({job="swag", service="${service}", host=~"$host"} | path=~"/_e/.+" | label_format event=`{{ regexReplaceAll "^/_e/([^/]+).*" .path "''${1}" }}` [$__interval]))'' "{{event}}")];
        })
      ];

      # Sign-ins and active users (neo_app_logins_total, neo_app_active_users).
      logins = {
        service,
        title ? "Sign-ins",
      }: let
        sel = appSelector service;
      in [
        (row title null)
        (stat {
          title = "Sign-ins";
          w = 4;
          h = 4;
          targets = [(pq ''sum(increase(neo_app_logins_total{${sel}, result="success"}[$__range])) or vector(0)'' "")];
          decimals = 0;
          options.colorMode = "none";
        })
        (stat {
          title = "Failed sign-ins";
          w = 4;
          h = 4;
          targets = [(pq ''sum(increase(neo_app_logins_total{${sel}, result="failure"}[$__range])) or vector(0)'' "")];
          decimals = 0;
          thresholds = thresholdsGreenRed [
            {
              color = "orange";
              value = 1;
            }
            {
              color = "red";
              value = 10;
            }
          ];
        })
        (stat {
          title = "Active users (24 h)";
          w = 4;
          h = 4;
          targets = [(pq ''max(neo_app_active_users{${sel}, window="24h"})'' "")];
          noValue = "–";
          options.colorMode = "none";
        })
        (bars {
          title = "Sign-ins";
          w = 12;
          h = 8;
          targets = [(pq "sum by (result, method) (increase(neo_app_logins_total{${sel}}[$__interval]))" "{{method}} {{result}}")];
        })
      ];

      # What people use (neo_app_feature_used_total).
      features = {
        service,
        title ? "Features used",
      }: let
        sel = appSelector service;
      in [
        (row title null)
        (bargauge {
          title = "Most used";
          w = 12;
          h = 10;
          description = "How often each feature was used in the chosen time range.";
          targets = [(pq "sort_desc(round(sum by (feature) (increase(neo_app_feature_used_total{${sel}}[$__range]))))" "{{feature}}")];
        })
        (bars {
          title = "Use over time";
          w = 12;
          h = 10;
          targets = [(pq "sum by (feature) (increase(neo_app_feature_used_total{${sel}}[$__interval]))" "{{feature}}")];
        })
      ];

      # LLM calls: requests, tokens (uncached input on its own), cache share,
      # cost, latency and failures by provider/model.
      llm = {
        service,
        title ? "AI",
      }: let
        sel = appSelector service;
        tok = kind: ''sum(increase(neo_app_llm_tokens_total{${sel}, kind="${kind}"}[$__range])) or vector(0)'';
        small = t: q: d:
          stat {
            title = t;
            w = 4;
            h = 4;
            description = d;
            targets = [(pq q "")];
            unit = "short";
            options.colorMode = "none";
          };
      in [
        (row title null)
        (small "Uncached input tokens" (tok "input") "Prompt tokens the provider charged at the full price (not read from its cache).")
        (small "Cached input tokens" (tok "cache_read") "Prompt tokens read from the provider's cache (cheap).")
        (small "Output tokens" (tok "output") "Tokens the model wrote.")
        (stat {
          title = "Cache hit rate";
          w = 4;
          h = 4;
          description = "Share of prompt tokens read from the cache.";
          unit = "percentunit";
          targets = [(pq "(${tok "cache_read"}) / clamp_min((${tok "cache_read"}) + (${tok "input"}) + (${tok "cache_write"}), 1)" "")];
          options.colorMode = "none";
        })
        (stat {
          title = "Cost";
          w = 4;
          h = 4;
          unit = "currencyUSD";
          decimals = 2;
          targets = [(pq "sum(increase(neo_app_llm_cost_usd_total{${sel}}[$__range])) or vector(0)" "")];
          options.colorMode = "none";
        })
        (stat {
          title = "Failed AI calls";
          w = 4;
          h = 4;
          targets = [(pq ''sum(increase(neo_app_llm_requests_total{${sel}, status!="ok"}[$__range])) or vector(0)'' "")];
          decimals = 0;
          thresholds = red;
        })
        (bars {
          title = "Uncached input tokens by model";
          w = 12;
          h = 8;
          targets = [(pq ''sum by (model) (increase(neo_app_llm_tokens_total{${sel}, kind="input"}[$__interval]))'' "{{model}}")];
        })
        (bars {
          title = "Tokens by kind";
          w = 12;
          h = 8;
          targets = [(pq "sum by (kind) (increase(neo_app_llm_tokens_total{${sel}}[$__interval]))" "{{kind}}")];
        })
        (bars {
          title = "AI calls by result";
          w = 12;
          h = 8;
          targets = [(pq "sum by (status, model) (increase(neo_app_llm_requests_total{${sel}}[$__interval]))" "{{model}} {{status}}")];
        })
        (timeseries {
          title = "AI answer time (95th percentile)";
          w = 12;
          h = 8;
          unit = "s";
          targets = [(pq "histogram_quantile(0.95, sum by (le, model) (rate(neo_app_llm_request_duration_seconds_bucket{${sel}}[$__rate_interval])))" "{{model}}")];
        })
        (table {
          title = "Tokens and cost by feature";
          w = 24;
          h = 7;
          targets = [
            (pq "sum by (feature, model, kind) (increase(neo_app_llm_tokens_total{${sel}}[$__range]))" ""
              // {
                instant = true;
                range = false;
                format = "table";
              })
          ];
          transformations = [
            {
              id = "groupingToMatrix";
              options = {
                columnField = "kind";
                rowField = "feature";
                valueField = "Value";
              };
            }
          ];
        })
      ];

      # Background jobs (neo_app_job_*).
      jobs = {
        service,
        title ? "Jobs",
      }: let
        sel = appSelector service;
      in [
        (row title null)
        (table {
          title = "Last success";
          w = 12;
          h = 8;
          unit = "s";
          targets = [
            (pq "max by (job) (time() - neo_app_job_last_success_timestamp_seconds{${sel}})" ""
              // {
                instant = true;
                range = false;
                format = "table";
              })
          ];
          transformations = [
            {
              id = "organize";
              options = {
                excludeByName.Time = true;
                renameByName.Value = "ago";
              };
            }
          ];
        })
        (bars {
          title = "Runs";
          w = 12;
          h = 8;
          targets = [(pq "sum by (job, status) (increase(neo_app_job_runs_total{${sel}}[$__interval]))" "{{job}} {{status}}")];
        })
      ];

      # Errors: app-counted errors and every error log line, with the lines.
      errors = {
        service,
        containers,
        # Host units of the app (a timer's service, …), names with or without .service.
        units ? [],
        title ? "Errors",
        # LogQL pipeline that keeps error lines, for apps that do not log JSON
        # with `level` (third-party images), e.g. ''|~ "(?i)\\b(error|exception)\\b"''.
        filter ? null,
        # false for apps without a metrics endpoint (no neo_app_errors_total).
        metrics ? true,
      }: let
        sel = appSelector service;
        lines = errorLines {inherit containers units filter;};
      in
        [
          (row title null)
          (bars {
            title = "Error log lines";
            w =
              if metrics
              then 12
              else 24;
            h = 8;
            datasource = loki;
            targets = [(lqLegend ''sum by (unit) (count_over_time(${lines} [$__interval]))'' "{{unit}}")];
          })
        ]
        ++ lib.optional metrics (bars {
          title = "Errors by component";
          w = 12;
          h = 8;
          targets = [(pq "sum by (component, kind) (increase(neo_app_errors_total{${sel}}[$__interval]))" "{{component}} {{kind}}")];
        })
        ++ [
          (logs {
            title = "Error lines";
            w = 24;
            h = 12;
            targets = [(lq lines)];
          })
        ];
    };

    # Alert rules for the standard metrics; merge into
    # (Summaries read $values.B through `with`: without data, e.g. noData =
    # "alerting", it is missing, the template fails and Grafana sends the raw text.)
    # neo.monitoring.apps.<service>.alerts. Only what the app exports:
    # turn on llm / logins / jobs / data as they apply.
    monitoring.appAlerts = {
      service,
      # Shown in titles: "<label>: AI provider failing".
      label,
      containers ? [],
      units ? [],
      errorFilter ? null,
      # Error log lines in 15 minutes before `errors` fires; null = no rule.
      errorThreshold ? 10,
      llm ? false,
      logins ? false,
      # job name → seconds since the last success before `job-<name>` fires.
      jobs ? {},
      # source → seconds since the last update before `data-<source>` fires.
      data ? {},
    }: let
      s = alertSelector service;
      # How long a job's (or data source's) metric may be missing on every
      # server before `*-missing` fires: its own limit, at least 6 hours, so
      # a restart or a move to another server never trips it.
      # A PromQL duration that also reads well in the summary (6h, 2d, 90m).
      missingWindow = maxAge: let
        sec = lib.max maxAge 21600;
      in
        if lib.mod sec 86400 == 0
        then "${toString (sec / 86400)}d"
        else if lib.mod sec 3600 == 0
        then "${toString (sec / 3600)}h"
        else if lib.mod sec 60 == 0
        then "${toString (sec / 60)}m"
        else "${toString sec}s";
    in
      lib.optionalAttrs (errorThreshold != null && (containers != [] || units != [])) {
        errors = {
          title = "${label}: errors in the log";
          datasource = "loki";
          expr = "sum by (host) (count_over_time(${errorLines {
            inherit containers units;
            hostSel = "";
            filter = errorFilter;
          }} [15m]))";
          above = errorThreshold;
          for = "0s";
          summary = "${label} on {{ $labels.host }} logged {{ with $values.B }}{{ printf \"%.0f\" .Value }}{{ else }}many{{ end }} errors in the last 15 minutes.";
        };
      }
      // lib.optionalAttrs llm {
        llm-failing = {
          title = "${label}: AI provider failing";
          # Share of failed calls per provider, once there were at least 3.
          expr = ''(sum by (host, provider) (increase(neo_app_llm_requests_total{${s}, status!="ok"}[15m])) / sum by (host, provider) (increase(neo_app_llm_requests_total{${s}}[15m]))) and on (host, provider) sum by (host, provider) (increase(neo_app_llm_requests_total{${s}, status!="ok"}[15m])) >= 3'';
          above = 0.5;
          for = "5m";
          severity = "critical";
          summary = "{{ with $values.B }}{{ humanizePercentage .Value }}{{ else }}Most{{ end }} of AI calls to {{ $labels.provider }} from ${label} failed in the last 15 minutes (API down, key revoked or out of credit).";
        };
        llm-auth = {
          title = "${label}: AI key rejected";
          expr = ''sum by (host, provider) (increase(neo_app_llm_requests_total{${s}, status="auth"}[15m]))'';
          above = 0;
          for = "0s";
          severity = "critical";
          summary = "{{ $labels.provider }} rejected the API key of ${label}: check the key and the account's credit.";
        };
      }
      // lib.optionalAttrs logins {
        login-failures = {
          title = "${label}: many failed sign-ins";
          expr = ''sum by (host) (increase(neo_app_logins_total{${s}, result="failure"}[15m]))'';
          above = 10;
          for = "0s";
          summary = "{{ with $values.B }}{{ printf \"%.0f\" .Value }}{{ else }}Many{{ end }} failed sign-ins to ${label} in the last 15 minutes.";
        };
      }
      // lib.concatMapAttrs (job: maxAge: let
        sel = ''neo_app_job_last_success_timestamp_seconds{${s}, job="${job}"}'';
      in {
        # A restart, a deploy or a move to another server leaves no series
        # for a while (the app reports again after its next run): no data
        # is OK here, and the age must stay too high for 15 minutes.
        "job-${job}" = {
          title = "${label}: ${job} did not succeed";
          expr = "max by (host) (time() - ${sel})";
          above = maxAge;
          for = "15m";
          summary = "${label}: ${job} on {{ $labels.host }} {{ with $values.B }}last succeeded {{ humanizeDuration .Value }} ago{{ else }}is late{{ end }}.";
        };
        # The job reported no success on any server for a long time (never
        # ran, its metric is gone): the rule above sees no data then.
        "job-${job}-missing" = {
          title = "${label}: ${job} reports nothing";
          expr = "absent_over_time(${sel}[${missingWindow maxAge}])";
          above = 0;
          for = "15m";
          summary = "${label}: ${job} has reported no success on any server for ${missingWindow maxAge} (it never ran, or its numbers no longer arrive).";
        };
      })
      jobs
      // lib.concatMapAttrs (source: maxAge: let
        sel = ''neo_app_data_updated_timestamp_seconds{${s}, source="${source}"}'';
      in {
        # Same as the jobs: no data right after a restart is OK.
        "data-${source}" = {
          title = "${label}: ${source} data is old";
          expr = "max by (host) (time() - ${sel})";
          above = maxAge;
          for = "15m";
          summary = "${label}: ${source} on {{ $labels.host }} {{ with $values.B }}was last updated {{ humanizeDuration .Value }} ago{{ else }}is late{{ end }}.";
        };
        "data-${source}-missing" = {
          title = "${label}: ${source} data reports nothing";
          expr = "absent_over_time(${sel}[${missingWindow maxAge}])";
          above = 0;
          for = "15m";
          summary = "${label}: ${source} has reported no update on any server for ${missingWindow maxAge} (it never ran, or its numbers no longer arrive).";
        };
      })
      data;
  };
}
