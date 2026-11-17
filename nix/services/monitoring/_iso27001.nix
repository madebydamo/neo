# ISO/IEC 27001:2022 evidence dashboards and alert rules (folder "ISO 27001").
# Not a module: imported by _grafana.nix with its panel helpers (`h`); one
# file per dashboard in _iso27001/.
#
# They collect technical evidence for Annex A controls that monitoring can
# show. They do not make anyone compliant: the ISMS (scope, risk assessment,
# Statement of Applicability, policies, reviews) stays an organisational task.
#
# Data beyond the Neo dashboards (monitoring-agent/_alloy.nix):
#   sudo commands as {event="privileged", provider="sudo"} (username, target,
#   command as structured metadata) and neo_auth_events_total{event="privileged"};
#   node_systemd_timer_last_trigger_seconds (timers), node_timex_* (clock);
#   loki_write_* / prometheus_remote_storage_* (agent delivery).
{
  lib,
  h,
}: let
  inherit (h) lqLegend dashboard fsFilter rule;
  folder = "neo-iso27001";

  iso = args:
    dashboard args
    // {
      tags = ["neo" "iso27001"];
      links = [
        {
          title = "ISO 27001";
          type = "dashboards";
          tags = ["iso27001"];
          asDropdown = true;
          includeVars = false;
          keepTime = true;
        }
      ];
    };

  red = v: {
    color = "red";
    value = v;
  };
  orange = v: {
    color = "orange";
    value = v;
  };

  instantLoki = expr: legend: (lqLegend expr legend) // {queryType = "instant";};

  # Units a person ran sudo from (SSH or console session), not services.
  interactive = ''unit="session.scope"'';

  switchLines = ''{job="journal", unit="nixos-rebuild-switch-to-configuration.service"} |= "switching to system configuration"'';

  # Timers that never ran report 0: left out.
  timerAgeHours = names: "(time() - (node_systemd_timer_last_trigger_seconds{name=~\"${names}\"} > 0)) / 3600";

  daysLeft = ''node_filesystem_avail_bytes{${fsFilter}} / clamp_min(-deriv(node_filesystem_avail_bytes{${fsFilter}}[1d]), 0.001) / 86400'';

  isoRule = args:
    rule ({
        inherit folder;
        group = "iso27001";
      }
      // args);
in {
  folders.${folder}.title = "ISO 27001";

  # One file per dashboard in _iso27001/, with the helpers above.
  dashboards = lib.mapAttrs (_: dashboard: {inherit folder dashboard;}) (lib.genAttrs' ["overview" "logging" "access" "changes" "capacity" "network"] (name:
    lib.nameValuePair "iso-${name}" (import ./_iso27001/${name}.nix {
      g = h // {inherit iso red orange instantLoki interactive switchLines timerAgeHours daysLeft;};
    })));

  alertRules = [
    (isoRule {
      uid = "neo-iso-clock";
      title = "Clock not synchronised";
      expr = "1 - node_timex_sync_status";
      for = "30m";
      summary = "The clock of {{ $labels.host }} has not been synchronised for 30 minutes; log times may be wrong (A.8.17).";
    })
    (isoRule {
      uid = "neo-iso-backup-overdue";
      title = "Backup overdue";
      expr = timerAgeHours "backup.timer";
      above = 26;
      for = "0s";
      summary = "The last backup on {{ $labels.host }} started {{ printf \"%.0f\" $values.B.Value }} hours ago (A.8.13).";
    })
    (isoRule {
      uid = "neo-iso-logs-dropped";
      title = "Logs are being lost";
      expr = "sum by (host) (increase(loki_write_dropped_entries_total[15m]))";
      for = "0s";
      summary = "{{ $labels.host }} could not deliver {{ printf \"%.0f\" $values.B.Value }} log lines in 15 minutes (A.8.15).";
    })
    (isoRule {
      uid = "neo-iso-logs-silent";
      title = "No logs from a server";
      expr = "sum by (host) (increase(loki_write_sent_entries_total[15m])) == bool 0";
      for = "15m";
      summary = "{{ $labels.host }} has sent no log lines for 30 minutes (A.8.15).";
    })
  ];
}
