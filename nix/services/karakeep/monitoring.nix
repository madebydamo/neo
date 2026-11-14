# Karakeep metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# The karakeep container runs the web app and the background workers:
#   - web    :3000/api/metrics  users, bookmarks, assets, queues, tRPC and HTTP
#   - workers :9091/metrics     worker job results, crawler status codes and latency
# Both answer only with `Authorization: Bearer $PROMETHEUS_AUTH_TOKEN`; without
# that variable Karakeep picks a random token and skips the Node.js metrics.
# The workers' server listens on 127.0.0.1 and a random port by default:
# WORKERS_HOST / WORKERS_PORT open it to the agent while it runs.
# Meilisearch serves :7700/metrics behind its master key once the experimental
# metrics feature is on.
#
# With vpn.enabled, karakeep lives in the vpn container's network namespace, so
# the agent scrapes the vpn container on Karakeep's network (the port joins
# vpn.ports for the conflict check).
#
# /api/metrics is on Karakeep's public port and under the ^/api/ tinyauth
# public path: hidePaths answers 404 for it on the internet, always.
{...}: {
  flake.modules.nixos.karakeep-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.karakeep;
    app = config.neo.monitoring.apps.karakeep;
    collecting = config.neo.monitoring.collecting;
    sel = lib.neo.grafana.appSelector "karakeep";
    net = lib.neo.docker.serviceNetwork "karakeep";
    vpn = cfg.vpn.enabled or false;
    workersPort = 9091;

    # Container that owns Karakeep's address on its network.
    karakeepHost =
      if vpn
      then "vpn"
      else "karakeep";

    # The workers counter is named without _total; accept both spellings.
    workerStats = match: ''{__name__=~"karakeep_worker_stats(_total)?", ${sel}, ${match}}'';
  in {
    # Outside the mkIf on cfg.enabled: a definition of neo.services.karakeep
    # under a condition that reads it would recurse.
    config = lib.mkMerge [
      {neo.services.karakeep.vpn.ports = lib.mkIf collecting (lib.mkOptionDefault [workersPort]);}
      (lib.mkIf cfg.enabled {
        virtualisation.oci-containers.containers = lib.mkIf collecting {
          karakeep.environment = {
            WORKERS_HOST = "0.0.0.0";
            WORKERS_PORT = toString workersPort;
          };
          karakeep-meilisearch.environment.MEILI_EXPERIMENTAL_ENABLE_METRICS = "true";
        };

        neo.core.secrets.containerEnv.karakeep = lib.mkIf collecting {
          PROMETHEUS_AUTH_TOKEN = app.scrapeToken;
        };

        neo.monitoring.apps.karakeep = {
          hidePaths = ["/api/metrics"];

          metrics = {
            web = {
              container = karakeepHost;
              network = net;
              port = cfg.port;
              path = "/api/metrics";
              auth = "bearer";
            };
            workers = {
              container = karakeepHost;
              network = net;
              port = workersPort;
              auth = "bearer";
            };
            meilisearch = {
              container = "karakeep-meilisearch";
              network = net;
              port = 7700;
              auth = "bearer";
              # Required secret (asserted in default.nix); a placeholder on encrypted configs.
              token = toString cfg.meiliMasterKey;
            };
          };

          dashboards.overview = {
            title = "Karakeep";
            description = "Saved bookmarks, background jobs (page copies, AI tags, search), the web app and its search engine.";
            time = "now-7d";
            panels = with lib.neo.grafana; [
              (row "Library" null)
              (stat {
                title = "Running";
                w = 4;
                h = 4;
                description = "The web app, the background workers and the search engine all answer.";
                targets = [(pq "min(up{${sel}})" "")];
                mappings = upDown;
                thresholds = {
                  mode = "absolute";
                  steps = [
                    {
                      color = "red";
                      value = null;
                    }
                    {
                      color = "green";
                      value = 1;
                    }
                  ];
                };
              })
              (stat {
                title = "Users";
                w = 4;
                h = 4;
                targets = [(pq "max(karakeep_total_users{${sel}})" "")];
                thresholds = thresholdsGreenRed [];
              })
              (stat {
                title = "Bookmarks";
                w = 4;
                h = 4;
                description = "Links, notes and images saved by all users.";
                targets = [(pq "max(karakeep_total_bookmarks{${sel}})" "")];
                thresholds = thresholdsGreenRed [];
              })
              (stat {
                title = "Saved files";
                w = 4;
                h = 4;
                description = "Space used by page copies, screenshots, images and uploads.";
                targets = [(pq "max(karakeep_total_asset_size_bytes{${sel}})" "")];
                unit = "bytes";
                thresholds = thresholdsGreenRed [];
              })
              (stat {
                title = "Waiting jobs";
                w = 4;
                h = 4;
                description = "Background jobs (page copies, AI tags, search) not done yet.";
                targets = [(pq ''sum(karakeep_queue_jobs{${sel}, status=~"pending|pending_retry|running"}) or vector(0)'' "")];
                thresholds = thresholdsGreenRed [
                  {
                    color = "orange";
                    value = 100;
                  }
                ];
              })
              (stat {
                title = "Failed jobs";
                w = 4;
                h = 4;
                description = "Background jobs that gave up and stay failed.";
                targets = [(pq ''sum(karakeep_queue_jobs{${sel}, status="failed"}) or vector(0)'' "")];
                thresholds = thresholdsGreenRed [
                  {
                    color = "red";
                    value = 1;
                  }
                ];
              })
              (timeseries {
                title = "Bookmarks over time";
                targets = [
                  (pq "max(karakeep_total_bookmarks{${sel}})" "bookmarks")
                ];
              })
              (timeseries {
                title = "Saved files size";
                targets = [(pq "max(karakeep_total_asset_size_bytes{${sel}})" "files")];
                unit = "bytes";
              })
              (bars {
                title = "Bookmarks added, by where they came from";
                description = "Web page, browser extension, mobile app, RSS feeds, imports, …";
                w = 24;
                h = 7;
                targets = [(pq "sum by (source) (increase(karakeep_bookmark_creations_total{${sel}}[$__interval]))" "{{source}}")];
              })

              (row "Background jobs" null)
              (timeseries {
                title = "Queues";
                description = "Jobs per queue that are waiting, retrying or running.";
                targets = [(pq ''sum by (queue_name) (karakeep_queue_jobs{${sel}, status=~"pending|pending_retry|running"})'' "{{queue_name}}")];
              })
              (timeseries {
                title = "Failed jobs per queue";
                targets = [(pq ''sum by (queue_name) (karakeep_queue_jobs{${sel}, status="failed"})'' "{{queue_name}}")];
              })
              (bars {
                title = "Jobs done";
                targets = [(pq ''sum by (worker_name) (increase(${workerStats ''status="completed"''}[$__interval]))'' "{{worker_name}}")];
              })
              (bars {
                title = "Jobs failed";
                description = "Failed attempts; \"failed_permanent\" means the job gave up.";
                targets = [(pq ''sum by (worker_name, status) (increase(${workerStats ''status!="completed"''}[$__interval]))'' "{{worker_name}} {{status}}")];
              })
              (bars {
                title = "Answers from crawled websites";
                description = "HTTP status codes of the pages Karakeep fetched (200 is fine, 4xx/5xx mean the site refused or failed).";
                targets = [(pq "sum by (status_code) (increase(karakeep_crawler_status_codes_total{${sel}}[$__interval]))" "{{status_code}}")];
              })
              (timeseries {
                title = "Time until a page is saved";
                description = "From adding a link to having its copy (median and 95%).";
                targets = [
                  (pq "histogram_quantile(0.5, sum by (le) (rate(karakeep_bookmark_crawl_latency_seconds_bucket{${sel}}[$__rate_interval])))" "median")
                  (pq "histogram_quantile(0.95, sum by (le) (rate(karakeep_bookmark_crawl_latency_seconds_bucket{${sel}}[$__rate_interval])))" "95%")
                ];
                unit = "s";
              })

              (row "Web app" null)
              (timeseries {
                title = "Requests by answer";
                description = "Requests to the API used by the browser extension, the apps and the web page.";
                targets = [(pq "sum by (status) (rate(karakeep_http_requests_total{${sel}}[$__rate_interval]))" "{{status}}")];
                unit = "reqps";
              })
              (timeseries {
                title = "Slowest API routes (95%)";
                targets = [(pq ''topk(10, histogram_quantile(0.95, sum by (route, le) (rate(karakeep_http_request_duration_seconds_bucket{${sel}}[$__rate_interval]))))'' "{{route}}")];
                unit = "s";
              })
              (bars {
                title = "Web page errors";
                description = "Calls from the web page that failed, by call and reason.";
                targets = [(pq "sum by (path, code) (increase(karakeep_trpc_errors_total{${sel}}[$__interval]))" "{{path}} {{code}}")];
              })
              (timeseries {
                title = "Slowest web page calls (95%)";
                targets = [(pq ''topk(10, histogram_quantile(0.95, sum by (path, le) (rate(karakeep_trpc_request_duration_seconds_bucket{${sel}}[$__rate_interval]))))'' "{{path}}")];
                unit = "s";
              })
              (timeseries {
                title = "Responsiveness";
                description = "How long the web app and the workers keep work waiting (event loop delay, 99%). High values feel like a slow app.";
                targets = [(pq "max by (scrape) (karakeep_nodejs_eventloop_lag_p99_seconds{${sel}})" "{{scrape}}")];
                unit = "s";
              })

              (row "Search" null)
              (bargauge {
                title = "Search entries per index";
                description = "Bookmarks the search engine knows about.";
                targets = [(pq "sum by (index) (meilisearch_index_docs_count{${sel}})" "{{index}}")];
                thresholds = thresholdsGreenRed [];
              })
              (timeseries {
                title = "Search database size";
                description = "Disk space of the search engine, and the part of it in use.";
                targets = [
                  (pq "max(meilisearch_db_size_bytes{${sel}})" "on disk")
                  (pq "max(meilisearch_used_db_size_bytes{${sel}})" "in use")
                ];
                unit = "bytes";
              })
              (stateTimeline {
                title = "Indexing";
                description = "When the search engine was busy taking in changes.";
                w = 24;
                h = 4;
                targets = [(pq "max(meilisearch_is_indexing{${sel}})" "indexing")];
                mappings = [
                  {
                    type = "value";
                    options = {
                      "0" = {
                        text = "Idle";
                        color = "transparent";
                        index = 0;
                      };
                      "1" = {
                        text = "Indexing";
                        color = "blue";
                        index = 1;
                      };
                    };
                  }
                ];
              })
              (timeseries {
                title = "Search task wait";
                description = "How long the oldest waiting task has waited for the search engine.";
                targets = [(pq "max(meilisearch_task_queue_latency_seconds{${sel}})" "wait")];
                unit = "s";
              })
              (timeseries {
                title = "Search tasks";
                description = "Tasks by state (failed tasks are kept for a while).";
                targets = [(pq ''sum by (value) (meilisearch_nb_tasks{${sel}, kind="statuses", value=~"enqueued|processing|failed"})'' "{{value}}")];
              })

              (row "Processes and containers" null)
              (timeseries {
                title = "Processor";
                targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name=~"karakeep.*"}[$__rate_interval]))'' "{{name}}")];
                unit = "percentunit";
              })
              (timeseries {
                title = "Memory";
                targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name=~"karakeep.*"})'' "{{name}}")];
                unit = "bytes";
              })
              (logs {
                title = "Errors and warnings";
                w = 24;
                h = 10;
                targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-karakeep.*"} |~ "(?i)(error|warn|exception|fail)"'')];
              })
            ];
          };
        };
      })
    ];
  };
}
