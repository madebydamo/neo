# Gitea metrics and dashboard (neo.monitoring.apps, nix/modules/core/monitoring.nix).
#
# Gitea's /metrics endpoint is off by default: turned on while a monitoring
# agent runs on this machine, protected with the app's scrape token
# (Authorization: Bearer). It is served on Gitea's web port, so SWAG answers
# /metrics with 404 on every Gitea site (hidePaths), whether or not it is on.
#
# Metric names checked against modules/metrics/collector.go at v28.1.0
# (image gitea/gitea:28): gitea_users{state="active|inactive"},
# gitea_repositories, gitea_organizations, gitea_issues_open/_closed (the issue
# table, so pull requests are counted too; there is no separate PR metric),
# gitea_releases, gitea_mirrors, gitea_webhooks, gitea_hooktasks, … plus the
# default Go and process collectors. ENABLED_ISSUE_BY_LABEL/_REPOSITORY stay
# off (one series per label / repository).
{...}: {
  flake.modules.nixos.gitea-monitoring = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.gitea;
    app = config.neo.monitoring.apps.gitea;
    sel = lib.neo.grafana.appSelector "gitea";

    # metric: name, optionally with extra matchers ("gitea_users" or
    # ''gitea_users{state="active"}'').
    count = title: description: metric: let
      parts = builtins.match "([a-z_]+)(\\{(.*)})?" metric;
      extra = builtins.elemAt parts 2;
      matchers =
        if extra == null
        then sel
        else "${sel}, ${extra}";
    in
      lib.neo.grafana.stat {
        inherit title description;
        w = 4;
        h = 4;
        targets = [(lib.neo.grafana.pq "max(${builtins.head parts}{${matchers}})" "")];
        options.colorMode = "value";
        thresholds = lib.neo.grafana.thresholdsGreenRed [];
      };
  in {
    config = lib.mkIf cfg.enabled {
      virtualisation.oci-containers.containers.gitea.environment = lib.mkIf config.neo.monitoring.collecting {
        GITEA__metrics__ENABLED = "true";
      };
      neo.core.secrets.containerEnv.gitea = lib.mkIf config.neo.monitoring.collecting {
        GITEA__metrics__TOKEN = app.scrapeToken;
      };

      neo.monitoring.apps.gitea = {
        hidePaths = ["/metrics"];

        metrics.main = {
          container = "gitea";
          port = cfg.port;
          interval = "60s";
          auth = "bearer";
        };

        dashboards.overview = {
          title = "Gitea";
          description = "Accounts, repositories, issues and webhooks of Gitea, and how much the server uses.";
          time = "now-7d";
          panels = with lib.neo.grafana; [
            (row "Overview" null)
            (stat {
              title = "Running";
              w = 4;
              h = 4;
              description = "Gitea answers.";
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
            (count "Users" "Accounts that are activated and can sign in." ''gitea_users{state="active"}'')
            (count "Repositories" "All repositories, including mirrors." "gitea_repositories")
            (count "Organizations" "Groups of people that share repositories." "gitea_organizations")
            (count "Open issues" "Open issues and pull requests together." "gitea_issues_open")
            (count "Mirrors" "Repositories that copy another one automatically." "gitea_mirrors")

            (row "Content" null)
            (timeseries {
              title = "Users";
              description = "Inactive accounts are not activated yet (for example still waiting for the e-mail confirmation) or were turned off by an admin.";
              targets = [(pq "max by (state) (gitea_users{${sel}})" "{{state}}")];
              decimals = 0;
            })
            (timeseries {
              title = "Repositories and organizations";
              targets = [
                (pq "max(gitea_repositories{${sel}})" "repositories")
                (pq "max(gitea_organizations{${sel}})" "organizations")
                (pq "max(gitea_mirrors{${sel}})" "mirrors")
                (pq "max(gitea_releases{${sel}})" "releases")
              ];
              decimals = 0;
            })
            (timeseries {
              title = "Issues and pull requests";
              description = "Gitea counts pull requests as issues, so both are in these numbers.";
              targets = [
                (pq "max(gitea_issues_open{${sel}})" "open")
                (pq "max(gitea_issues_closed{${sel}})" "closed")
              ];
              decimals = 0;
            })
            (timeseries {
              title = "Webhooks";
              description = "Webhooks set up on repositories and organizations, and the deliveries Gitea has recorded for them.";
              targets = [
                (pq "max(gitea_webhooks{${sel}})" "webhooks")
                (pq "max(gitea_hooktasks{${sel}})" "deliveries")
              ];
              decimals = 0;
            })

            (row "Server" null)
            (timeseries {
              title = "Gitea memory";
              description = "Memory the Gitea program holds and how much of it is in use.";
              targets = [
                (pq "max(go_memstats_sys_bytes{${sel}})" "reserved")
                (pq "max(go_memstats_heap_inuse_bytes{${sel}})" "in use")
              ];
              unit = "bytes";
            })
            (timeseries {
              title = "Parallel tasks";
              description = "Tasks running at the same time inside Gitea (Go goroutines). A number that only grows points to a leak.";
              targets = [(pq "max(go_goroutines{${sel}})" "tasks")];
              decimals = 0;
            })
            (timeseries {
              title = "Gitea processor";
              description = "Processor time the Gitea program uses (1 = one full core).";
              targets = [(pq "sum(rate(process_cpu_seconds_total{${sel}}[$__rate_interval]))" "cpu")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Container processor";
              targets = [(pq ''sum by (name) (rate(container_cpu_usage_seconds_total{host=~"$host", name="gitea"}[$__rate_interval]))'' "{{name}}")];
              unit = "percentunit";
            })
            (timeseries {
              title = "Container memory";
              targets = [(pq ''sum by (name) (container_memory_working_set_bytes{host=~"$host", name="gitea"})'' "{{name}}")];
              unit = "bytes";
            })
            (logs {
              title = "Errors and warnings";
              w = 24;
              h = 10;
              targets = [(lq ''{job="journal", host=~"$host", unit=~"docker-gitea(\\.service)?"} |~ "(?i)(error|warn|exception|fail)"'')];
            })
          ];
        };
      };
    };
  };
}
