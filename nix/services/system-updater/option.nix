# System auto-updater: scheduled neo update/activate.
# Config repo bootstrap is neo-bootstrap, owned by the neo web service.
{...}: {
  flake.modules.nixos.system-updater-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.system-updater = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "automatic updates of the server system and Neo" {
                rank = 0;
                default = true;
              };
              schedule = mkOption {
                type = types.str;
                default = "*-*-* 04:00:00";
                # systemd OnCalendar expression
                description = "When automatic updates run. The default is 04:00 every day (systemd calendar time, e.g. daily or *-*-* 04:00:00).";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Update time";};
              };
              activationMode = mkOption {
                type = types.enum ["switch" "boot"];
                default = "switch";
                # switch: live nixos-rebuild switch. boot: nixos-rebuild boot only (avoids
                # unattended restarts of dhcpcd/journald). Only affects scheduled runs; interactive
                # neo activate / web Activate stay on switch unless --mode boot / NEO_ACTIVATION_MODE=boot.
                description = "Live switch applies updates right away. On next restart waits until the server restarts, which is safer on servers that freeze during updates.";
                rank = 15;
                ui = lib.neo.ui.mkUi {
                  label = "Activation mode";
                  summary = true;
                  choiceLabels = {
                    switch = "Live switch";
                    boot = "On next restart";
                  };
                };
              };
              rebootAfterBoot = mkOption {
                type = types.bool;
                default = true;
                # Scheduled runs only: after a successful boot activate that advanced the system
                # profile, systemd-run schedules a reboot 60s later (lets the oneshot finish history).
                # Skipped on no-op/failed updates; ignored in switch mode and for web Activate.
                description = "Restart the server about a minute after an update so it takes effect. Only used with Activation mode On next restart, and only when something changed.";
                rank = 16;
                ui = lib.neo.ui.mkUi {
                  label = "Restart after update";
                  visibleWhen = "activationMode=boot";
                };
              };
              garbageCollectOlderThan = mkOption {
                type = types.nullOr types.str;
                default = "30d";
                # nix-collect-garbage --delete-older-than, run before each auto-update; null skips GC.
                # When set, also enables nix keep-outputs so build-time deps survive GC.
                description = "Delete old system versions after this long (e.g. 30d) to free disk space. Older versions can no longer be rolled back to.";
                rank = 20;
                ui = lib.neo.ui.mkUi {
                  label = "Delete old versions after";
                };
              };
            }
            // lib.neo.mkSystemdUnits ["neo-auto-update"]
            // lib.neo.mkSystemdTimers ["neo-auto-update"]
            // lib.neo.mkAppdata (lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata).systemHistoryDir
            // lib.neo.mkServiceMeta {
              category = "Core";
              icon = ./icon.svg;
              tagline = "Keeps your homeserver up to date";
              description = ''
                Installs system and Neo updates automatically on a schedule, and can clean up old versions to free disk space.
              '';
              notes = ''
                A systemd timer runs neo update and activate, with optional nix garbage collection.
                activationMode=boot builds and sets the next boot default without a live switch (safer on boxes where an unattended switch freezes networking); rebootAfterBoot (default on) then restarts the host so the new version runs.
                The config repo is created by neo-bootstrap, which stays enabled when this timer is off. CLI path and neo input settings live under neo-cli.
              '';
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "System auto-updater configuration";
      };
    };
}
