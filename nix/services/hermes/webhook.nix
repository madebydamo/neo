# Hermes inbound webhooks: routes other modules declare in
# neo.services.hermes.webhook.routes (e.g. Grafana alerts from monitoring).
#
# The HMAC key is generated once at activation into webhook.secretFile (raw, for
# senders such as Grafana's provisioning `$__file{}`) and webhook.env (dotenv,
# appended to $HERMES_HOME/.env as WEBHOOK_SECRET, the routes' shared secret).
# The listener binds all interfaces on webhook.port; the firewall keeps it to
# this host and the neo-host Docker bridge (lib.neo.docker.mkHostAccess),
# which senders such as Grafana join.
{...}: {
  flake.modules.nixos.hermes-webhook = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.hermes;
    wh = cfg.webhook;
    on = cfg.enabled && wh.routes != {};
    dir = dirOf wh.secretFile;
    envFile = "${dir}/webhook.env";
    settings = {
      platforms.webhook = {
        enabled = true;
        extra = {
          inherit (wh) port;
          inherit (wh) routes;
        };
      };
    };
  in {
    config = lib.mkIf on {
      services.hermes-agent.settings = settings;
      services.hermes-agent.environment.WEBHOOK_ENABLED = "true";
      services.hermes-agent.environmentFiles = [envFile];

      # config.yaml changes do not restart the gateway (it may be running the
      # switch). New or changed routes are rare and need the listener rebuilt.
      systemd.services.hermes-agent.restartTriggers = [(builtins.toJSON settings)];

      system.activationScripts.hermes-webhook-secret = lib.stringAfter ["users" "groups" "hermes-reown"] ''
        d=${lib.escapeShellArg dir}
        ${pkgs.coreutils}/bin/install -d -m 0750 -o hermes -g homeserver "$d"
        if [ ! -s ${lib.escapeShellArg wh.secretFile} ]; then
          (umask 027; ${pkgs.openssl}/bin/openssl rand -hex 32 | ${pkgs.coreutils}/bin/tr -d '\n' > ${lib.escapeShellArg wh.secretFile})
        fi
        (umask 077; printf 'WEBHOOK_SECRET=%s\n' "$(${pkgs.coreutils}/bin/cat ${lib.escapeShellArg wh.secretFile})" > ${lib.escapeShellArg envFile})
        ${pkgs.coreutils}/bin/chown hermes:homeserver ${lib.escapeShellArg wh.secretFile}
        ${pkgs.coreutils}/bin/chmod 0640 ${lib.escapeShellArg wh.secretFile}
        ${pkgs.coreutils}/bin/chown root:root ${lib.escapeShellArg envFile}
      '';
      system.activationScripts.hermes-agent-setup.deps = ["hermes-webhook-secret"];
    };
  };
}
