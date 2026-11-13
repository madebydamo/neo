# Monitoring agent options: Grafana Alloy sends this machine's logs, numbers
# and uptime checks to a Monitoring server (this one or another).
{...}: {
  flake.modules.nixos.monitoring-agent-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.monitoring-agent = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Sends this server's logs, health and uptime checks to your Monitoring dashboards" {rank = 0;};

              serverUrl = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Web address of the Monitoring dashboards on another server, e.g. https://grafana.example.com. Leave empty when Monitoring runs on this server.";
                rank = 10;
                example = "https://grafana.example.com";
                ui = lib.neo.ui.mkUi {label = "Monitoring server";};
              };

              # Sent as Bearer to <serverUrl>/ingest/… (monitoring/swag.nix there).
              token = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "The ingest token from the Monitoring settings on that server. Not needed when Monitoring runs on this server.";
                rank = 20;
                ui = lib.neo.ui.mkUi {
                  label = "Ingest token";
                  visibleWhen = "serverUrl";
                };
              };

              name = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Name this server has on the dashboards. Empty uses its host name.";
                rank = 30;
                ui = lib.neo.ui.mkUi {label = "Server name";};
              };

              uptimeChecks = mkOption {
                type = types.bool;
                default = true;
                description = "Check every 30 seconds that the apps on this server open at their web address, for the uptime charts and alerts.";
                rank = 40;
                ui = lib.neo.ui.mkUi {label = "Check app uptime";};
              };

              extraUptimeTargets = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "More web addresses to check from here, e.g. apps on your other servers or your router. Checking another server from here shows whether it can be reached from outside.";
                rank = 41;
                example = ["https://photos.example.com" "http://192.168.1.1"];
                ui = lib.neo.ui.mkUi {
                  label = "Also check";
                  visibleWhen = "uptimeChecks";
                };
              };

              # Alloy's own HTTP server (metrics, UI), loopback only.
              port = mkOption {
                type = types.port;
                default = 12345;
                description = "Port of the agent's status page on this server.";
                rank = 80;
                ui = lib.neo.ui.mkUi {
                  label = "Agent port";
                  group = lib.neo.ui.groups.advanced;
                };
              };
            }
            // lib.neo.mkContainerDefinitions {
              alloy = "grafana/alloy:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/monitoring-agent"
            // lib.neo.mkServiceMeta {
              category = "Monitoring";
              license = {
                spdx = "Apache-2.0";
                url = "https://github.com/grafana/alloy/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Reports this server to your Monitoring dashboards";
              description = ''
                A small helper that sends this server's logs, sign-ins, web traffic, processor, memory and disk use to your Monitoring dashboards, and checks that its apps open.
                Turn it on on every server you want to watch, including the one that runs Monitoring.
              '';
              notes = ''
                Grafana Alloy in a container (host network, read-only access to the journal, /proc, /sys, Docker and SWAG's logs).
                Sends: the whole systemd journal (container logs included), SWAG access logs as JSON, tinyauth and SSH sign-ins, node exporter and cAdvisor numbers, systemd unit states, and blackbox checks of this server's app addresses.
                On another server: set Monitoring server and Ingest token. Its apps' Reachable from must include where this server connects from (Internet for a remote one).
                While it runs, tinyauth logs in JSON with its audit stream on.
              '';
              projectUrl = "https://grafana.com/oss/alloy-opentelemetry-collector/";
              githubUrl = "https://github.com/grafana/alloy";
              releaseUrl = "https://github.com/grafana/alloy/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Monitoring agent configuration";
      };
    };
}
