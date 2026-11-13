# Hermes in Herdr: the desktop app reaches this server with `hc machine add`
# (ssh as admin) and its "Start an agent" menu runs `hermes` there. Plain
# `hermes` as admin would write into HERMES_HOME (owned by hermes) as the
# wrong user, so admin gets a `hermes` shim first in its PATH that runs the
# real CLI as the hermes user: same memory, skills, .env and AI login as the
# gateway. hc itself is copied there by `hc machine add --install` (a static
# release build, always the client's version), so Neo does not package it.
#
# Herdr shows an agent on its board once it reports its state as OSC 7501 on
# its terminal. Hermes does not do that itself: the herdr-status plugin
# (./herdr-status) does, from the hc terminal ids the shim passes through sudo.
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
    # a terminal UI needs (TERM survives env_reset on its own) and the hc
    # terminal ids the status plugin reports to (the rule has SETENV).
    hermesShim = pkgs.writeShellApplication {
      name = "hermes";
      text = ''
        exec /run/wrappers/bin/sudo --preserve-env=HC_TERMINAL_ID,HC_TERMINAL_SOCKET -u hermes -- \
          ${hermesCli}/bin/neo-hermes-cli "$@"
      '';
    };

    statusPlugin = pkgs.runCommand "herdr-status" {} ''
      mkdir -p $out
      cp ${./herdr-status/plugin.yaml} $out/plugin.yaml
      cp ${./herdr-status/__init__.py} $out/__init__.py
    '';

    # Plugins are opt-in (config.yaml plugins.enabled). Add ours without
    # touching the operator's own entries; never fails a switch.
    enablePlugin =
      pkgs.writers.writePython3 "hermes-enable-herdr-status" {
        libraries = [pkgs.python3Packages.ruamel-yaml];
      } ''
        import sys
        from pathlib import Path
        from ruamel.yaml import YAML

        path = Path(sys.argv[1])
        yaml = YAML()
        data = (yaml.load(path) if path.exists() else None) or {}
        plugins = data.get("plugins")
        if not isinstance(plugins, dict):
            plugins = data["plugins"] = {}
        enabled = plugins.get("enabled")
        if not isinstance(enabled, list):
            enabled = plugins["enabled"] = []
        if "herdr-status" not in enabled:
            enabled.append("herdr-status")
            yaml.dump(data, path)
      '';
  in {
    config = lib.mkIf on {
      # hc runs its core as a user unit: without linger it (and every agent
      # in it) stops when the last ssh session closes.
      users.users.admin = {
        linger = true;
        packages = [hermesShim];
      };

      services.hermes-agent.extraPlugins = [statusPlugin];

      system.activationScripts.hermes-herdr-status = lib.stringAfter ["hermes-agent-setup"] ''
        ${pkgs.util-linux}/bin/runuser -u hermes -- ${enablePlugin} \
          ${lib.escapeShellArg "${cfg.stateDir}/.hermes/config.yaml"} \
          || echo "hermes: could not enable the herdr-status plugin" >&2
      '';

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
