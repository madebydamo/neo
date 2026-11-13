# Hermes in Herdr: the desktop app reaches this server with `hc machine add`
# (ssh as admin) and its "Start an agent" menu runs `hermes` there. Plain
# `hermes` as admin would write into HERMES_HOME (owned by hermes) as the
# wrong user, so admin gets a `hermes` shim first in its PATH that runs the
# real CLI as the hermes user: same memory, skills, .env and AI login as the
# gateway. hc itself is copied there by `hc machine add --install` (a static
# release build, always the client's version), so Neo does not package it.
#
# Gated by neo.services.hermes.herdr (default false).
{...}: {
  flake.modules.nixos.hermes-herdr = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.hermes;
    on = cfg.enabled && cfg.herdr;
    hermesPkg = config.services.hermes-agent.package;

    # Runs as hermes (sudo target). Same environment as the update supervisor.
    hermesCli = pkgs.writeShellApplication {
      name = "neo-hermes-cli";
      text = ''
        cd ${lib.escapeShellArg "${cfg.stateDir}/workspace"}
        export HOME=${lib.escapeShellArg cfg.stateDir}
        export HERMES_HOME=${lib.escapeShellArg "${cfg.stateDir}/.hermes"}
        exec ${hermesPkg}/bin/hermes "$@"
      '';
    };

    # The admin-side `hermes`. sudo drops most of the environment; keep what
    # a terminal UI needs (TERM survives env_reset on its own).
    hermesShim = pkgs.writeShellApplication {
      name = "hermes";
      text = ''
        exec /run/wrappers/bin/sudo -u hermes -- \
          ${hermesCli}/bin/neo-hermes-cli "$@"
      '';
    };
  in {
    config = lib.mkIf on {
      # hc runs its core as a user unit: without linger it (and every agent
      # in it) stops when the last ssh session closes.
      users.users.admin = {
        linger = true;
        packages = [hermesShim];
      };

      # admin → hermes for the CLI only. hermes already has full sudo and
      # admin shares root's authorizedKeys, so this grants nothing new.
      security.sudo.extraRules = lib.neo.mkSudoExtraRules {
        users = ["admin"];
        runAs = "hermes";
        commands = [
          {
            package = hermesCli;
            name = "neo-hermes-cli";
          }
        ];
      };
    };
  };
}
