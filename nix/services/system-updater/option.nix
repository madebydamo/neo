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
              enabled = mkEnableOption "System auto-updater (scheduled neo update/activate)" {
                rank = 0;
                default = true;
              };
              schedule = mkOption {
                type = types.str;
                default = "*-*-* 04:00:00";
                description = "systemd OnCalendar schedule for auto-update (e.g. daily, *-*-* 04:00:00)";
                rank = 10;
              };
              activationMode = mkOption {
                type = types.enum ["switch" "boot"];
                default = "switch";
                description = ''
                  How scheduled activate applies the new generation.
                  switch (default): live nixos-rebuild switch — running system becomes the new generation immediately.
                  boot: nixos-rebuild boot only — sets the boot default without a live switch (avoids unattended restarts of dhcpcd/journald and similar freezes on edge boxes). Pair with rebootAfterBoot to finish applying on a delayed host restart. Interactive neo activate and the web Activate button stay on switch unless you pass --mode boot / NEO_ACTIVATION_MODE=boot.
                '';
                rank = 15;
                ui = lib.neo.ui.mkUi {
                  label = "Activation mode";
                  summary = true;
                  choiceLabels = {
                    switch = "Live switch";
                    boot = "Boot (no live switch)";
                  };
                };
              };
              rebootAfterBoot = mkOption {
                type = types.bool;
                default = true;
                description = ''
                  When activationMode is boot and a scheduled run advances the system profile (reboot_pending), schedule a delayed host restart so the new generation becomes /run/current-system. Default true for unattended edge boxes. Skipped on no-op updates, failed activate, and always ignored when activationMode is switch (live path already runs the new gen). Does not affect interactive web Activate. Delay (~60s) lets the oneshot finish history and OnSuccess supervise hooks start first.
                '';
                rank = 16;
                ui = lib.neo.ui.mkUi {
                  label = "Restart host after boot activate";
                };
              };
              garbageCollectOlderThan = mkOption {
                type = types.nullOr types.str;
                default = "30d";
                description = "Value passed to nix-collect-garbage --delete-older-than at the end of each auto-update (e.g. 30d, 60d); set to null to skip GC. When set, also enables nix keep-outputs so build-time deps (e.g. Rust crate builds) survive GC while their generation is still live";
                rank = 20;
                ui = lib.neo.ui.mkUi {
                  label = "Garbage collect older than";
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
