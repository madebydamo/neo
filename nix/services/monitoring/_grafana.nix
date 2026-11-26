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
        uid = "neo-cert-name";
        title = "Certificate does not fit the address";
        # The agent checks every address twice: once normally and once with
        # certificate verification off (monitoring-agent/_alloy.nix, job
        # uptime-tls). The second one working while the first does not means
        # the server answers and only its certificate is wrong: a name it does
        # not cover, an expired one or a chain that does not verify.
        # "App does not answer" fires as well, with the same cause.
        expr = ''probe_success{job="uptime-tls"} == 1 unless on (host, target) probe_success{job="uptime"} == 1'';
        for = "15m";
        summary = "The HTTPS certificate of {{ $labels.target }} is not valid for that address (wrong name, expired or an incomplete chain). The app itself answers, checked from {{ $labels.probe_from }}.";
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
      (rule {
        uid = "neo-log-errors";
        title = "Many errors in the log";
        # Error lines of the whole server, whatever wrote them: the per-app
        # rules only see the units an app declares. Journal priority err and
        # worse (`level`, from the priority keyword). The threshold is far
        # above a normal day (25 Nov 2026: at most 157 lines an hour, on the
        # busiest of five servers).
        # `or on () vector(0)` keeps a series when there is no error line at
        # all; without it the rule flips between NoData and Normal.
        datasource = "loki";
        expr = ''sum by (host) (count_over_time({job="journal", level=~"err|error|crit|alert|emerg"} [1h])) or on () vector(0)'';
        above = 500;
        for = "15m";
        summary = "{{ $labels.host }} wrote {{ with $values.B }}{{ printf \"%.0f\" .Value }}{{ else }}many{{ end }} error lines in the last hour. The Logs dashboard shows which units.";
      })
      (rule {
        uid = "neo-log-flood";
        title = "A service floods the log";
        # One unit writing so much that the journal grows and real errors
        # drown in it (uster, 26 Nov 2026: 3.9 GB journal, 47 000 lines a day
        # from two containers). Per unit, not per server: the server total is
        # the sum of all its apps and says nothing.
        # 6 000 lines in 15 minutes is more than twice the loudest unit today.
        # A 15-minute window, not an hour: this query reads every journal
        # stream, and it runs every minute.
        datasource = "loki";
        expr = ''sum by (host, unit) (count_over_time({job="journal"} [15m]))'';
        above = 6000;
        for = "15m";
        summary = "{{ $labels.unit }} on {{ $labels.host }} wrote {{ with $values.B }}{{ printf \"%.0f\" .Value }}{{ else }}very many{{ end }} log lines in 15 minutes: it fills the journal and hides real errors.";
      })
      (rule {
        uid = "neo-coredump";
        title = "A program crashed";
        # systemd-coredump saved a core dump: "Process 524301 (chromium) of
        # user 61102 dumped core." (brugg, 26 Nov 2026). The program name
        # comes out of the line, the unit label does not have it (one unit
        # instance per dump).
        datasource = "loki";
        expr = ''sum by (host, program) (count_over_time({job="journal", unit=~"systemd-coredump.*"} |= "dumped core" | regexp "Process [0-9]+ [(](?P<program>[^)]+)[)]" [1h])) or on () vector(0)'';
        above = 0;
        for = "0s";
        summary = "{{ $labels.program }} on {{ $labels.host }} crashed and wrote a core dump ({{ with $values.B }}{{ printf \"%.0f\" .Value }}{{ else }}one or more{{ end }} in the last hour). `coredumpctl` on that server shows the dumps.";
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
