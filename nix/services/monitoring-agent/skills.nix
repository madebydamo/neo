# Hermes skill for monitoring-agent.
{...}: {
  flake.modules.nixos.monitoring-agent-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.monitoring-agent;
  in {
    config.neo.services.monitoring-agent.skill.conf = lib.neo.mkServiceSkill {
      service = "monitoring-agent";
      inherit config;
      description = "Grafana Alloy agent sending this server's logs and metrics to Monitoring";
      tags = ["neo" "monitoring" "alloy"];
      body = ''
        ## When to Use
        A server is missing on the Monitoring dashboards, logs or numbers stop, uptime checks look wrong.

        ## Architecture notes
        - Container `alloy` (host network, root). Status page and its own metrics on 127.0.0.1:${toString cfg.port}
          (`curl -s 127.0.0.1:${toString cfg.port}/-/ready`, component health at `/api/v0/web/components`).
        - Sends to the Monitoring server: loopback when Monitoring runs here, else `<serverUrl>/ingest/…` with the ingest token
          (401 = wrong token, 403 = no token set on the server, 404 at SWAG = the vhost's Reachable from does not include this server).
        - Sources: journal (docker container output via docker-<c>.service), SWAG `log/nginx/neo-access.log` (JSON, written while the agent runs),
          tinyauth audit (JSON logging on while the agent runs) and sshd sign-ins, node exporter + systemd + textfile `neo_service_unit`,
          cAdvisor, blackbox checks of apps reachable from the Internet plus `extraUptimeTargets`.

        ## Credentials
        - `services.monitoring-agent.serverUrl` and `token` (the server's `services.monitoring.ingestToken`).

        ## Verification
        - `docker logs alloy 2>&1 | grep -iE "error|warn" | tail`
        - On the Monitoring server: `curl -s 127.0.0.1:3100/loki/api/v1/label/host/values` lists this server's name.
      '';
    };
  };
}
