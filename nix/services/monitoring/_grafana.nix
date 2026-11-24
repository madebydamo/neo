# Grafana dashboards and alert rules Neo provisions (not a module: imported by
# default.nix; import-tree skips `_` files). One file per dashboard in
# _dashboards/ (ISO 27001: _iso27001.nix, _iso27001/).
#
# Metric and label names come from the agent (monitoring-agent/_alloy.nix):
#   node_* (job=node), container_* (job=containers, label name),
#   probe_* (job=uptime, label target), neo_service_unit{service,name,kind},
#   neo_auth_events_total{event,result,provider,attempt}, neo_http_requests_total{service,status_class},
#   neo_http_request_duration_seconds; every series carries host.
# Loki streams: job=journal {unit, level}, job=swag {service, status_class},
#   login events {event, result, provider} with username/ip as structured metadata;
#   attempt="scan" on sshd failures with an unknown user name on a keys-only sshd.
{lib}: let
  # Panel helpers: nix/lib/grafana.nix.
  inherit (lib.neo.grafana) prom loki pq pqInstant lq lqLegend pos thresholdsGreenRed panel stat timeseries bars bargauge logs upDown stateTimeline table text row queryVar hostVar dashboard;

  fsFilter = ''fstype!~"tmpfs|ramfs|devtmpfs|overlay|squashfs|nsfs|fuse.*|autofs|9p",mountpoint!~"/(nix/store|boot)"'';

  # Alert rules: query A (Prometheus, instant) → B (last value; summaries read
  # $values.B, raw queries are not in $values) → C (threshold on B). Each
  # query returns one series per thing that is wrong.
  # The rule body is lib.neo.grafana.alertRule (also used by app alerts).
  rule = {
    folder ? "neo",
    group ? "neo",
    ...
  } @ args:
    lib.neo.grafana.alertRule (removeAttrs args ["folder" "group"])
    // {
      folderUID = folder;
      ruleGroup = group;
      orgID = 1;
    };

  alertRules =
    [
      (rule {
        uid = "neo-app-down";
        title = "App does not answer";
        # host = the machine that runs the app (app_host), not the one that
        # checks it: the Monitoring server checks the other machines' apps
        # too, and its host label read as "this server is down". probe_from
        # keeps the checker, so the two checks of one app stay two series;
        # the notification policy groups by host, so per app machine.
        # Probes without app_host keep their host.
        expr = ''label_replace(1 - probe_success, "host", "$1", "app_host", "(.+)")'';
        for = "5m";
        severity = "critical";
        summary = "{{ with $labels.app }}{{ . }} on {{ $labels.host }} ({{ $labels.target }}){{ else }}{{ $labels.target }}{{ end }} has not answered for 5 minutes (checked from {{ or $labels.probe_from $labels.host }}).";
      })
      (rule {
        uid = "neo-server-silent";
        title = "Server stopped reporting";
        # Expected: every machine whose agent sent its list to this server
        # (neo_monitoring_expected_host, monitoring-expected-hosts in
        # default.nix), so a machine that is down stays alerting until it
        # reports again or its file in agents/ is deleted. Agents from before
        # that list: machines seen in the last day.
        expr = ''(group by (host) (label_replace(neo_monitoring_expected_host, "host", "$1", "expected_host", "(.+)")) or group by (host) (max_over_time(node_boot_time_seconds[1d]))) unless group by (host) (node_boot_time_seconds)'';
        for = "5m";
        severity = "critical";
        summary = "{{ $labels.host }} has sent no numbers for over 5 minutes: it is off, offline or its monitoring agent stopped. The alert stays until it reports again.";
      })
      (rule {
        uid = "neo-cert-expiry";
        title = "Certificate expires soon";
        # Days left below 10 (SWAG renews 30 days ahead).
        expr = "10 - (min by (app, target) (probe_ssl_earliest_cert_expiry) - time()) / 86400";
        for = "1h";
        summary = "The HTTPS certificate of {{ $labels.target }} expires in less than 10 days and was not renewed.";
      })
      (rule {
        uid = "neo-disk-full";
        title = "Disk almost full";
        expr = "100 * (1 - node_filesystem_avail_bytes{${fsFilter}} / node_filesystem_size_bytes{${fsFilter}})";
        above = 90;
        for = "15m";
        summary = "{{ $labels.mountpoint }} on {{ $labels.host }} is {{ printf \"%.0f\" $values.B.Value }}% full.";
      })
      (rule {
        uid = "neo-unit-failed";
        title = "Service failed";
        expr = ''node_systemd_unit_state{state="failed"} == 1'';
        for = "10m";
        summary = "{{ $labels.name }} on {{ $labels.host }} has failed.";
      })
      (rule {
        uid = "neo-unit-restarting";
        title = "Service keeps restarting";
        # A crash loop is "activating" most of the time and "failed" only for
        # moments, so "Service failed" alone misses it (issue #31). Counts
        # automatic restarts only (NRestarts): a switch or a manual restart
        # does not add to it.
        expr = "increase(node_systemd_service_restart_total[15m])";
        above = 3;
        for = "5m";
        summary = "{{ $labels.name }} on {{ $labels.host }} restarted {{ with $values.B }}{{ printf \"%.0f\" .Value }}{{ else }}many{{ end }} times in 15 minutes: it starts and crashes again.";
      })
      (rule {
        uid = "neo-failed-logins";
        title = "Many failed sign-ins";
        # Without attempt="scan" (unknown sshd user names on a keys-only
        # sshd, monitoring-agent/_alloy.nix): they cannot succeed. Ends below
        # 5: increase() reads 10 events as 9.3 or 10.3, which flapped.
        expr = ''sum by (host, provider) (increase(neo_auth_events_total{event="login",result="failure",attempt!="scan"}[15m]))'';
        above = 10;
        recover = 5;
        for = "0s";
        summary = "{{ with $values.B }}{{ printf \"%.0f\" .Value }}{{ else }}Many{{ end }} failed {{ $labels.provider }} sign-ins on {{ $labels.host }} in the last 15 minutes.";
      })
      (rule {
        uid = "neo-server-errors";
        title = "App answers with errors";
        expr = ''(sum by (host, service) (rate(neo_http_requests_total{status_class="5xx"}[10m])) / sum by (host, service) (rate(neo_http_requests_total[10m]))) and on (host, service) sum by (host, service) (rate(neo_http_requests_total{status_class="5xx"}[10m])) > 0.02'';
        above = 0.2;
        for = "10m";
        summary = "{{ with $values.B }}{{ humanizePercentage .Value }}{{ else }}Many{{ end }} of requests to {{ $labels.service }} on {{ $labels.host }} end in a server error.";
      })
    ]
    ++ iso.alertRules;

  # What the dashboard files (_dashboards/, _iso27001/) get as `g`.
  helpers = {
    inherit prom loki pq pqInstant lq lqLegend pos thresholdsGreenRed panel stat timeseries bars bargauge logs upDown stateTimeline table row queryVar hostVar dashboard fsFilter rule text;
  };
  iso = import ./_iso27001.nix {
    inherit lib;
    h = helpers;
  };
in {
  # Grafana folders (uid → title) and dashboards (name → folder + model).
  folders =
    {
      neo.title = "Neo";
    }
    // iso.folders;
  dashboards =
    lib.mapAttrs (_: dashboard: {
      folder = "neo";
      inherit dashboard;
    }) {
      overview = import ./_dashboards/overview.nix {g = helpers;};
      server = import ./_dashboards/server.nix {g = helpers;};
      logs = import ./_dashboards/logs.nix {g = helpers;};
      logins = import ./_dashboards/logins.nix {g = helpers;};
    }
    // iso.dashboards;
  inherit alertRules;
}
