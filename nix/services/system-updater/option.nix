# System auto-updater: bootstrap config repo + scheduled neo update/activate.
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
              enabled = mkEnableOption "System auto-updater (bootstrap config repo + scheduled neo update/activate)" {
                rank = 0;
                default = true;
              };
              schedule = mkOption {
                type = types.str;
                default = "*-*-* 04:00:00";
                description = "systemd OnCalendar schedule for auto-update (e.g. daily, *-*-* 04:00:00)";
                rank = 10;
              };
              garbageCollectOlderThen = mkOption {
                type = types.nullOr types.str;
                default = "30d";
                description = "Value passed to nix-collect-garbage --delete-older-than at the end of each auto-update (e.g. 30d, 60d); set to null to skip GC. When set, also enables nix keep-outputs so build-time deps (e.g. Rust crate builds) survive GC while their generation is still live";
                rank = 20;
              };
            }
            // lib.neo.mkSystemdUnits ["neo-bootstrap" "neo-auto-update"]
            // lib.neo.mkAppdata (lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata).systemHistoryDir
            // lib.neo.mkServiceMeta {
              category = "Core";
              icon = "https://api.iconify.design/mdi/update.svg";
              description = ''
                Keeps the homeserver configuration repository bootstrapped and periodically runs neo update + activate so system packages and Neo modules stay current.
                When enabled, the config repo is initialized if missing and a systemd timer runs scheduled upgrades (with optional nix garbage collection).
                CLI path/template settings live under neo-cli (this service always uses the server profile for configPath). This option only controls whether automatic system updates run.
              '';
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "System auto-updater configuration";
      };
    };
}
