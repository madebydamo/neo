# Monitoring server options: Grafana (dashboards, alerts), Prometheus (numbers)
# and Loki (logs). Monitoring agents on this and other machines send to it.
{...}: {
  flake.modules.nixos.monitoring-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.monitoring = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Central dashboards for the logs, uptime and health of all your servers, with alerts" {rank = 0;};

              # Bearer token SWAG checks on /ingest/* (Loki push, Prometheus remote write).
              ingestToken = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Lets your other servers send their logs and health data here. Press Generate and enter the same token in the Monitoring agent on each of them; leave empty when only this server reports. Changing it later stops the other servers' reports until you update them too.";
                rank = 10;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Ingest token";};
              };

              logRetentionDays = mkOption {
                type = types.ints.positive;
                default = 30;
                description = "How many days logs are kept. Older logs are deleted.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Keep logs for (days)";};
              };

              metricRetentionDays = mkOption {
                type = types.ints.positive;
                default = 90;
                description = "How many days uptime and health numbers are kept for the charts. Older numbers are deleted.";
                rank = 30;
                ui = lib.neo.ui.mkUi {label = "Keep charts for (days)";};
              };

              # Grafana webhook contact point → Hermes webhook route (hermes/webhook.nix).
              assistantAlerts = mkOption {
                type = types.bool;
                default = true;
                description = "Send alerts (an app is down, a server stops reporting, a disk runs full, many failed logins) to you through the assistant on Telegram. Needs the assistant on this server.";
                rank = 40;
                ui = lib.neo.ui.mkUi {label = "Alerts through the assistant";};
              };

              assistantInvestigates = mkOption {
                type = types.bool;
                default = false;
                description = "Let the assistant look into each alert (logs, status) before it messages you. Each alert then uses your AI provider.";
                rank = 41;
                ui = lib.neo.ui.mkUi {
                  label = "Assistant looks into alerts";
                  visibleWhen = "assistantAlerts";
                };
              };

              # Loopback ports the agent on this machine writes to (host network).
              lokiPort = mkOption {
                type = types.port;
                default = 3100;
                description = "Port on this server where the local agent delivers logs.";
                rank = 80;
                ui = lib.neo.ui.mkUi {
                  label = "Log port";
                  group = lib.neo.ui.groups.advanced;
                };
              };

              prometheusPort = mkOption {
                type = types.port;
                default = 9090;
                description = "Port on this server where the local agent delivers health numbers.";
                rank = 81;
                ui = lib.neo.ui.mkUi {
                  label = "Metrics port";
                  group = lib.neo.ui.groups.advanced;
                };
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "grafana";
            }
            // lib.neo.mkContainerDefinitions {
              grafana = "grafana/grafana:latest";
              prometheus = "prom/prometheus:v3";
              loki = "grafana/loki:3";
              extraUnits = ["grafana-seed"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/monitoring"
            // lib.neo.mkServiceMeta {
              category = "Monitoring";
              license = {
                spdx = "AGPL-3.0-only";
                url = "https://github.com/grafana/grafana/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Logs, uptime and alerts for all your servers";
              description = ''
                Collects the logs and health of this and your other servers in one place, with ready-made dashboards for uptime, app health, logins and web traffic.
                Search every log from one page and get a message when an app goes down or a disk runs full.
              '';
              notes = ''
                Grafana with Prometheus (numbers) and Loki (logs). Turn on Monitoring agent on every server that should report, including this one.
                Other servers send to https://<grafana subdomain>/ingest/… with the ingest token; those paths skip the homeserver login.
                Everyone who passes the homeserver login is a Grafana admin. Neo adds its dashboards (folders Neo and ISO 27001) and alert rules once; after that they are yours to change. Delete one to get Neo's version back on the next restart of Grafana.
              '';
              projectUrl = "https://grafana.com/oss/";
              githubUrl = "https://github.com/grafana/grafana";
              releaseUrl = "https://github.com/grafana/grafana/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Monitoring server configuration";
      };
    };
}
