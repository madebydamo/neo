# Grafana Alloy configuration of the monitoring agent (not a module: imported
# by default.nix; import-tree skips `_` files). Backtick strings are raw.
{
  lib,
  hostName,
  lokiUrl,
  prometheusUrl,
  tokenSet,
  vhosts,
  uptimeTargets,
  swagLogsOn,
  # neo.monitoring.scrapeTargets: {service, scrape, container|address, port, network, path, scheme, interval, auth, …}
  appTargets ? [],
  # target → env var holding its scrape credential (default.nix sets it in containerEnv.alloy)
  scrapeEnv,
}: let
  inherit (lib) concatMapStrings concatStringsSep optionalString;
  str = builtins.toJSON;
  # Target names end up in the job label: letters, digits and _ only.
  targetName = n: lib.strings.sanitizeDerivationName (builtins.replaceStrings ["://" "/" ":" "." "-"] ["_" "_" "_" "_" "_"] n);
  bearer = optionalString tokenSet ''
    bearer_token = sys.env("NEO_INGEST_TOKEN")
  '';

  # {{ if eq .vhost "a.example.com" }}a{{ else if … }}{{ else }}other{{ end }}
  serviceTemplate =
    if vhosts == []
    then "other"
    else
      concatStringsSep "{{ else }}" (map (v: ''{{ if eq .vhost "${v.host}" }}${v.service}'') vhosts)
      + "{{ else }}other"
      + concatMapStrings (_: "{{ end }}") vhosts;

  ident = s: builtins.replaceStrings ["-" "."] ["_" "_"] s;
  appComponent = t: "app_${ident t.service}_${ident t.scrape}";
  appLabels = t: ''
    rule {
      replacement  = ${str t.path}
      target_label = "__metrics_path__"
    }
    rule {
      replacement  = ${str t.scheme}
      target_label = "__scheme__"
    }
    rule {
      replacement  = ${str t.service}
      target_label = "service"
    }
    rule {
      replacement  = ${str t.scrape}
      target_label = "scrape"
    }
    rule {
      replacement  = ${str "app/${t.service}"}
      target_label = "job"
    }
  '';
  # Optional metric relabeling per endpoint (neo.monitoring.apps.*.metrics.*.relabel).
  fwd = t:
    if (t.relabel or []) == []
    then "prometheus.remote_write.neo.receiver"
    else "prometheus.relabel.${appComponent t}.receiver";
  relabelRule = r: let
    labelOnly = r.action == "labeldrop" || r.action == "labelkeep";
  in ''
    rule {
      ${optionalString (!labelOnly) "source_labels = ${str r.sourceLabels}"}
      regex         = ${str r.regex}
      action        = ${str r.action}
      ${optionalString (r.targetLabel != null) "target_label = ${str r.targetLabel}"}
      ${optionalString (r.action == "replace") "replacement = ${str r.replacement}"}
    }
  '';
  relabelBlock = t:
    optionalString ((t.relabel or []) != []) ''
      prometheus.relabel ${str (appComponent t)} {
        ${concatMapStrings relabelRule t.relabel}
        forward_to = [prometheus.remote_write.neo.receiver]
      }
    '';
  # Credentials, headers and query parameters of prometheus.scrape.
  scrapeExtra = t:
    optionalString (t.auth == "bearer") ''
      bearer_token = sys.env(${str (scrapeEnv t)})
    ''
    + optionalString (t.auth == "basic") ''
      basic_auth {
        username = ${str t.username}
        password = sys.env(${str (scrapeEnv t)})
      }
    ''
    + optionalString (t.headers != {}) ''
      http_headers = { ${concatStringsSep ", " (lib.mapAttrsToList (k: v: "${str k} = [${str v}]") t.headers)} }
    ''
    + optionalString (t.params != {}) ''
      params = { ${concatStringsSep ", " (lib.mapAttrsToList (k: v: "${str k} = ${str v}") t.params)} }
    '';
  appScrape = t: let
    c = appComponent t;
  in
    if t.container != null
    then ''
      // ${t.service} / ${t.scrape}: container ${t.container}:${toString t.port}${t.path}
      discovery.relabel ${str c} {
        targets = discovery.docker.apps.targets
        rule {
          source_labels = ["__meta_docker_container_name"]
          regex         = ${str "/${t.container}"}
          action        = "keep"
        }
        rule {
          source_labels = ["__meta_docker_network_name"]
          regex         = ${str t.network}
          action        = "keep"
        }
        rule {
          source_labels = ["__meta_docker_network_ip"]
          replacement   = ${str "$1:${toString t.port}"}
          target_label  = "__address__"
        }
        rule {
          source_labels = ["__address__"]
          target_label  = "instance"
        }
        ${appLabels t}
      }

      prometheus.scrape ${str c} {
        targets         = discovery.relabel.${c}.output
        scrape_interval = ${str t.interval}
        forward_to      = [${fwd t}]
        ${scrapeExtra t}
      }
      ${relabelBlock t}
    ''
    else ''
      // ${t.service} / ${t.scrape}: ${t.address}${t.path}
      discovery.relabel ${str c} {
        targets = [{ "__address__" = ${str t.address} }]
        ${appLabels t}
      }

      prometheus.scrape ${str c} {
        targets         = discovery.relabel.${c}.output
        scrape_interval = ${str t.interval}
        forward_to      = [${fwd t}]
        ${scrapeExtra t}
      }
      ${relabelBlock t}
    '';

  okCodes = lib.range 200 208 ++ lib.range 300 308 ++ [401 403];
  blackboxConfig = builtins.toJSON {
    modules.http_app = {
      prober = "http";
      timeout = "10s";
      http = {
        preferred_ip_protocol = "ip4";
        ip_protocol_fallback = true;
        follow_redirects = false;
        valid_status_codes = okCodes;
      };
    };
  };
in ''
  // Generated by neo (monitoring-agent). Edits are overwritten.

  loki.write "neo" {
    endpoint {
      url = ${str lokiUrl}
      ${bearer}
    }
    external_labels = { host = ${str hostName} }
  }

  prometheus.remote_write "neo" {
    endpoint {
      url = ${str prometheusUrl}
      ${bearer}
    }
    external_labels = { host = ${str hostName} }
  }

  // ── System log ────────────────────────────────────────────────────────────

  loki.relabel "journal" {
    forward_to = []

    // dockerd's journald-driver copy; docker-<c>.service carries the same lines.
    rule {
      source_labels = ["__journal_container_name"]
      regex         = ".+"
      action        = "drop"
    }
    rule {
      source_labels = ["__journal__systemd_unit"]
      target_label  = "unit"
    }
    // No unit (kernel, sessions): the syslog identifier.
    rule {
      source_labels = ["__journal__systemd_unit", "__journal_syslog_identifier"]
      separator     = ";"
      regex         = `^;(.+)$`
      target_label  = "unit"
    }
    // One value for all sign-in sessions (session-42.scope → session.scope).
    rule {
      source_labels = ["unit"]
      regex         = `session-[0-9]+\.scope`
      replacement   = "session.scope"
      target_label  = "unit"
    }
    rule {
      source_labels = ["__journal_priority_keyword"]
      target_label  = "level"
    }
  }

  loki.source.journal "journal" {
    path          = "/var/log/journal"
    max_age       = "12h"
    relabel_rules = loki.relabel.journal.rules
    labels        = { job = "journal" }
    forward_to    = [loki.process.journal.receiver]
  }

  loki.process "journal" {
    // Colour codes some apps print.
    stage.replace {
      expression = `(\x1b\[[0-9;]*m)`
      replace    = ""
    }

    // tinyauth audit stream (JSON while the agent runs).
    stage.match {
      selector = `{unit="docker-tinyauth.service"} |= "\"stream\":\"audit\""`
      stage.json {
        expressions = { event = "", result = "", username = "", provider = "", ip = "", reason = "" }
      }
      stage.labels {
        values = { event = "", result = "", provider = "" }
      }
      stage.structured_metadata {
        values = { username = "", ip = "", reason = "" }
      }
    }

    // sshd sign-ins.
    stage.match {
      selector = `{unit="sshd.service"} |~ "^Accepted "`
      stage.regex {
        expression = `^Accepted (?P<method>\S+) for (?P<username>\S+) from (?P<ip>\S+)`
      }
      stage.static_labels {
        values = { event = "login", result = "success", provider = "ssh" }
      }
      stage.structured_metadata {
        values = { username = "", ip = "", method = "" }
      }
    }
    stage.match {
      selector = `{unit="sshd.service"} |~ "^(Failed |Invalid user )"`
      stage.regex {
        expression = `^(?:Failed \S+ for (?:invalid user )?|Invalid user )(?P<username>\S+) from (?P<ip>\S+)`
      }
      stage.static_labels {
        values = { event = "login", result = "failure", provider = "ssh" }
      }
      stage.structured_metadata {
        values = { username = "", ip = "" }
      }
    }

    // sudo: "alice : TTY=pts/0 ; PWD=/ ; USER=root ; COMMAND=/bin/ls".
    stage.match {
      selector = `{job="journal"} |~ " ; COMMAND=" != "NOT in sudoers" != "command not allowed"`
      stage.regex {
        expression = `^\s*(?P<username>\S+) : .*?USER=(?P<target>\S+) ; COMMAND=(?P<command>.*)$`
      }
      stage.static_labels {
        values = { event = "privileged", result = "success", provider = "sudo" }
      }
      stage.structured_metadata {
        values = { username = "", target = "", command = "" }
      }
    }
    stage.match {
      selector = `{job="journal"} |~ "(NOT in sudoers|command not allowed|incorrect password attempts?) ; "`
      stage.regex {
        expression = `^\s*(?P<username>\S+) : (?P<reason>[^;]+) ;.*?(?:USER=(?P<target>\S+) ; COMMAND=(?P<command>.*))?$`
      }
      stage.static_labels {
        values = { event = "privileged", result = "failure", provider = "sudo" }
      }
      stage.structured_metadata {
        values = { username = "", target = "", command = "", reason = "" }
      }
    }

    stage.match {
      selector = `{event=~"login|privileged"}`
      stage.metrics {
        metric.counter {
          name        = "auth_events_total"
          prefix      = "neo_"
          description = "Sign-in attempts (tinyauth, sshd) and sudo commands."
          match_all   = true
          action      = "inc"
          max_idle_duration = "24h"
        }
      }
    }

    forward_to = [loki.write.neo.receiver]
  }

  ${optionalString swagLogsOn ''
    // ── Web requests (SWAG) ─────────────────────────────────────────────────

    local.file_match "swag" {
      path_targets = [{ "__path__" = "/var/log/swag/neo-access.log", job = "swag" }]
      sync_period  = "30s"
    }

    loki.source.file "swag" {
      targets       = local.file_match.swag.targets
      tail_from_end = true
      forward_to    = [loki.process.swag.receiver]
    }

    loki.process "swag" {
      stage.json {
        expressions = { time = "", vhost = "host", method = "", path = "uri", status = "", ip = "remote_addr", rt = "request_time" }
      }
      stage.timestamp {
        source = "time"
        format = "RFC3339"
      }
      stage.template {
        source   = "service"
        template = `${serviceTemplate}`
      }
      stage.template {
        source   = "status_class"
        template = `{{ printf "%.1s" .status }}xx`
      }
      stage.labels {
        values = { service = "", status_class = "" }
      }
      stage.label_drop {
        values = ["filename"]
      }
      stage.structured_metadata {
        values = { vhost = "", method = "", path = "", status = "", ip = "" }
      }
      stage.metrics {
        metric.counter {
          name        = "http_requests_total"
          prefix      = "neo_"
          description = "Web requests through SWAG."
          match_all   = true
          action      = "inc"
          max_idle_duration = "24h"
        }
        metric.histogram {
          name        = "http_request_duration_seconds"
          prefix      = "neo_"
          description = "Time SWAG took to answer."
          source      = "rt"
          buckets     = [0.01, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30]
          max_idle_duration = "24h"
        }
      }
      forward_to = [loki.write.neo.receiver]
    }
  ''}

  // ── Numbers ───────────────────────────────────────────────────────────────

  prometheus.exporter.unix "node" {
    procfs_path    = "/proc"
    sysfs_path     = "/sys"
    rootfs_path    = "/rootfs"
    udev_data_path = "/rootfs/run/udev/data"
    enable_collectors  = ["systemd", "textfile"]
    disable_collectors = ["mdadm", "ipvs", "btrfs", "infiniband", "xfs", "nfs", "nfsd", "fibrechannel", "tapestats", "bcache", "arp"]

    systemd {
      unit_include = `.+\.(service|timer)`
    }
    textfile {
      directory = "/etc/neo-textfile"
    }
    filesystem {
      mount_points_exclude = `^/(dev|proc|run|sys|var/lib/docker/.+|nix/store)($|/)`
      fs_types_exclude     = `^(autofs|binfmt_misc|bpf|cgroup2?|configfs|debugfs|devpts|devtmpfs|fusectl|hugetlbfs|iso9660|mqueue|nsfs|overlay|proc|procfs|pstore|rpc_pipefs|securityfs|selinuxfs|squashfs|sysfs|tracefs|tmpfs|ramfs|9p)$`
    }
    netclass {
      ignored_devices = `^(veth.*|br-.*|docker.*|lo)$`
    }
    netdev {
      device_exclude = `^(veth.*)$`
    }
  }

  discovery.relabel "node" {
    targets = prometheus.exporter.unix.node.targets
    rule {
      replacement  = "node"
      target_label = "job"
    }
  }

  prometheus.scrape "node" {
    targets         = discovery.relabel.node.output
    job_name        = "node"
    scrape_interval = "30s"
    forward_to      = [prometheus.remote_write.neo.receiver]
  }

  prometheus.exporter.cadvisor "containers" {
    docker_host      = "unix:///var/run/docker.sock"
    docker_only      = true
    storage_duration = "2m"
  }

  discovery.relabel "containers" {
    targets = prometheus.exporter.cadvisor.containers.targets
    rule {
      replacement  = "containers"
      target_label = "job"
    }
  }

  prometheus.scrape "containers" {
    targets         = discovery.relabel.containers.output
    job_name        = "containers"
    scrape_interval = "30s"
    forward_to      = [prometheus.relabel.containers.receiver]
  }

  prometheus.relabel "containers" {
    rule {
      source_labels = ["__name__"]
      regex         = "container_(cpu_usage_seconds_total|memory_working_set_bytes|network_receive_bytes_total|network_transmit_bytes_total|last_seen|start_time_seconds|spec_memory_limit_bytes)"
      action        = "keep"
    }
    rule {
      regex  = "id"
      action = "labeldrop"
    }
    forward_to = [prometheus.remote_write.neo.receiver]
  }

  // Counters derived from logs (sign-ins, web requests) live in Alloy's own registry.
  prometheus.exporter.self "alloy" { }

  prometheus.scrape "alloy" {
    targets         = prometheus.exporter.self.alloy.targets
    job_name        = "alloy"
    honor_labels    = true
    scrape_interval = "30s"
    forward_to      = [prometheus.relabel.alloy.receiver]
  }

  prometheus.relabel "alloy" {
    rule {
      source_labels = ["__name__"]
      regex         = "neo_.+|alloy_build_info|loki_write_(sent|dropped)_entries_total|prometheus_remote_storage_samples_(total|failed_total|dropped_total)"
      action        = "keep"
    }
    // loki_write_* name the Loki endpoint in "host"; keep it as "endpoint"
    // and let remote_write add this server's host label.
    rule {
      source_labels = ["__name__", "host"]
      regex         = "loki_write_.+;(.+)"
      target_label  = "endpoint"
    }
    rule {
      source_labels = ["__name__"]
      regex         = "loki_write_.+"
      target_label  = "host"
      replacement   = ""
    }
    rule {
      regex  = "component_id|component_path|filename"
      action = "labeldrop"
    }
    forward_to = [prometheus.remote_write.neo.receiver]
  }

  ${optionalString (uptimeTargets != []) ''
    // ── Uptime checks ───────────────────────────────────────────────────────

    prometheus.exporter.blackbox "uptime" {
      config = ${str blackboxConfig}
      ${concatMapStrings (t: ''
        target {
          name    = ${str (targetName t.name)}
          address = ${str t.address}
          module  = "http_app"
          labels  = { app = ${str t.name} }
        }
      '')
      uptimeTargets}
    }

    discovery.relabel "uptime" {
      targets = prometheus.exporter.blackbox.uptime.targets
      rule {
        source_labels = ["__param_target"]
        target_label  = "target"
      }
      rule {
        replacement  = "uptime"
        target_label = "job"
      }
    }

    prometheus.scrape "uptime" {
      targets         = discovery.relabel.uptime.output
      job_name        = "uptime"
      scrape_interval = "30s"
      scrape_timeout  = "15s"
      forward_to      = [prometheus.remote_write.neo.receiver]
    }
  ''}

  ${optionalString (appTargets != []) ''
    // ── App metrics (neo.monitoring.apps) ───────────────────────────────────

    ${optionalString (builtins.any (t: t.container != null) appTargets) ''
      // Every network of a container, not only its first: gluetun (vpn)
      // serves the apps routed through it on their own networks.
      discovery.docker "apps" {
        host                = "unix:///var/run/docker.sock"
        refresh_interval    = "30s"
        match_first_network = false
      }
    ''}
    ${concatMapStrings appScrape appTargets}
  ''}
''
