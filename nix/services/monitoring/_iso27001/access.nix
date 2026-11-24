# ISO 27001 dashboard (folder "ISO 27001"). Not a module: imported by
# _iso27001.nix with the panel helpers (`g`).
{g}: let
  inherit (g) bargauge bars instantLoki interactive iso logs loki lq pos pq table text thresholdsGreenRed;
in
  iso {
    uid = "neo-iso-access";
    title = "ISO 27001: Access and authentication";
    time = "now-30d";
    panels = [
      (text {
        gridPos = pos 0 0 24 3;
        content = ''
          **A.5.15 Access control, A.5.16 Identity management, A.5.17 Authentication information, A.5.18 Access rights, A.8.2 Privileged access rights, A.8.5 Secure authentication.**
          Sign-ins to the homeserver login (tinyauth) and SSH, and every sudo command. User names and addresses are stored with each event.
        '';
      })
      (bars {
        title = "Sign-ins";
        gridPos = pos 0 3 24 7;
        description = "ssh scan: attempts with a user name that does not exist on a server that accepts keys only (Internet scanners); they cannot succeed.";
        targets = [(pq ''sum by (provider, result) (label_replace(increase(neo_auth_events_total{event="login"}[$__interval]), "result", "scan", "attempt", "scan"))'' "{{provider}} {{result}}")];
      })
      (table {
        title = "Who signed in";
        gridPos = pos 0 10 12 9;
        description = "Successful sign-ins per user and method in the selected period. Review against the list of people who should have access (A.5.18).";
        datasource = loki;
        targets = [((instantLoki ''sum by (username, provider, host) (count_over_time({event="login", result="success"} [$__range]))'' "") // {format = "table";})];
        transformations = [
          {
            id = "organize";
            options = {
              excludeByName.Time = true;
              renameByName = {
                username = "User";
                provider = "Method";
                host = "Server";
                Value = "Sign-ins";
              };
            };
          }
        ];
      })
      (table {
        title = "Failed sign-ins";
        gridPos = pos 12 10 12 9;
        datasource = loki;
        description = "Every failed attempt. Kind scan: an SSH user name that does not exist on a server that accepts keys only (Internet scanners).";
        targets = [((instantLoki ''sum by (username, ip, reason, provider, attempt) (count_over_time({event="login", result="failure"} [$__range]))'' "") // {format = "table";})];
        transformations = [
          {
            id = "organize";
            options = {
              excludeByName.Time = true;
              renameByName = {
                username = "User";
                ip = "Address";
                reason = "Reason";
                provider = "Method";
                attempt = "Kind";
                Value = "Attempts";
              };
            };
          }
          {
            id = "sortBy";
            options.sort = [
              {
                field = "Attempts";
                desc = true;
              }
            ];
          }
        ];
      })
      (logs {
        title = "Locked accounts";
        gridPos = pos 0 19 12 8;
        targets = [(lq ''{event="login", result="failure"} | reason=~".*locked.*"'')];
      })
      (logs {
        title = "SSH sign-ins as root";
        gridPos = pos 12 19 12 8;
        targets = [(lq ''{event="login", provider="ssh"} | username="root"'')];
      })
      (bargauge {
        title = "Privileged commands by people";
        gridPos = pos 0 27 8 9;
        description = "sudo from sign-in sessions, per user.";
        datasource = loki;
        targets = [(instantLoki ''sum by (username, target) (count_over_time({event="privileged", ${interactive}} [$__range]))'' "{{username}} → {{target}}")];
        thresholds = thresholdsGreenRed [];
      })
      (logs {
        title = "Privileged commands by people";
        gridPos = pos 8 27 16 9;
        targets = [(lq ''{event="privileged", ${interactive}}'')];
      })
      (bargauge {
        title = "Privileged commands by services";
        gridPos = pos 0 36 8 9;
        description = "sudo used by Neo's own services (updates, backups, the web UI). Expected; review for unexpected units.";
        datasource = loki;
        targets = [(instantLoki ''sum by (unit) (count_over_time({event="privileged", unit!="session.scope"} [$__range]))'' "{{unit}}")];
        thresholds = thresholdsGreenRed [];
      })
      (logs {
        title = "Refused privileged commands";
        gridPos = pos 8 36 16 9;
        targets = [(lq ''{event="privileged", result="failure"}'')];
      })
    ];
  }
