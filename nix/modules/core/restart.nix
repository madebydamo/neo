# Updates that only take effect after a restart of the server.
#
#   neo-restart-required [--json] [SYSTEM]
#       exit 0 and print the reasons when SYSTEM (default: the system profile)
#       needs a restart to take full effect, exit 1 when it does not.
#
# Reasons, each compared against what actually runs, so nothing restarts for
# a change that already applied:
#
#   system        SYSTEM is not the running system (activationMode=boot: the
#                 new version is the boot default).
#   kernel        kernel, initrd, kernel modules or kernel boot options differ
#                 from the booted system.
#   docker        the running dockerd was started with another daemon.json than
#                 SYSTEM's. A switch restarts Docker when its settings change,
#                 but dockerd can come back from the old unit (a client hits
#                 docker.socket before systemd loads the new units; meilen,
#                 2026-11-15), and then only a restart applies the settings.
#   trigger:<id>  a component raised its restart trigger (below) above the
#                 level it had when it started in this boot.
#
# Triggers: core, a service or a plugin instance that ships a change only a
# restart applies raises its own counter, inside its `mkIf cfg.enabled`:
#
#   neo.core.restart.triggers.<id> = { level = 2; reason = "…"; };
#
# Every activation records in /run/neo/restart-baseline.json the level each
# trigger had when it first ran in this boot (/run is empty after a boot, so
# the first activation of a boot sets them all). A trigger is due when the
# level in SYSTEM is above that baseline. So a disabled service never counts,
# a service switched on later starts at its current level (unless `onEnable`),
# and the first rollout of a trigger restarts nothing.
#
# `neo activate` records the reasons; the system updater restarts the server
# for them (system-updater.rebootWhenRequired), once per system.
#
# Core trigger history (raise `level`, never lower it, add a line):
#   1  initial level
{...}: {
  flake.modules.nixos.core-restart = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.core.restart;
    manifest = pkgs.writeText "neo-restart.json" (builtins.toJSON {
      triggers = lib.mapAttrs (_: t: {inherit (t) level reason onEnable;}) cfg.triggers;
    });
    baseline = "/run/neo/restart-baseline.json";
    restartRequired = pkgs.writeShellScriptBin "neo-restart-required" ''
      PATH=${lib.makeBinPath [pkgs.coreutils pkgs.diffutils pkgs.gnugrep pkgs.jq pkgs.systemd]}:$PATH
      exec ${pkgs.bash}/bin/bash ${./restart-required.sh} "$@"
    '';
  in {
    options.neo.core.restart.triggers = lib.mkOption {
      type = lib.types.attrsOf (lib.types.submodule {
        options = {
          level = lib.mkOption {
            type = lib.types.ints.unsigned;
            description = "Raise by one for a change that needs a restart of the server on machines running this component.";
          };
          reason = lib.mkOption {
            type = lib.types.str;
            description = "One plain sentence: what the restart applies. Shown in the web UI, the updater log and Hermes' report.";
          };
          onEnable = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Also restart when the component is switched on (it needs something only a boot sets up).";
          };
        };
      });
      default = {};
      internal = true;
      visible = false;
      description = "Restart triggers of core, services and plugin instances; see nix/modules/core/restart.nix.";
    };

    config = {
      neo.core.restart.triggers.core = {
        level = 1;
        reason = "Neo core changed in a way that needs a restart.";
      };

      assertions =
        lib.mapAttrsToList (id: t: {
          assertion = builtins.match "[a-z][a-z0-9-]*(:[a-z0-9-]+)?" id != null && t.reason != "";
          message = "neo.core.restart.triggers.${id}: the id must be lowercase (letters, digits, -) and the reason must not be empty.";
        })
        cfg.triggers;

      system.systemBuilderCommands = ''
        ln -s ${manifest} $out/neo-restart.json
      '';

      # The level each trigger had when it started in this boot.
      system.activationScripts.neo-restart-baseline = {
        deps = [];
        text = ''
          mkdir -p /run/neo
          ${lib.getExe pkgs.jq} -n \
            --slurpfile new ${manifest} \
            --argjson base "$(cat ${baseline} 2>/dev/null || echo null)" \
            -f ${./restart-baseline.jq} > ${baseline}.tmp 2>/dev/null \
            && mv ${baseline}.tmp ${baseline} \
            || { echo "neo-restart-baseline: could not record restart levels" >&2; rm -f ${baseline}.tmp; }
        '';
      };
      environment.systemPackages = [restartRequired];
    };
  };
}
