# Docker image auto-updater service options (replacement for deprecated watchtower; uses central container registry).
{...}: {
  flake.modules.nixos.docker-updater-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.docker-updater = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Docker auto-updater (scheduled image pulls + restarts for declared containers)" {
                rank = 0;
                default = true;
              };
              schedule = mkOption {
                type = types.str;
                default = "Sun *-*-* 04:00:00";
                description = "systemd OnCalendar schedule for image checks (e.g. daily, weekly, Sun *-*-* 04:00:00)";
                rank = 10;
              };
            }
            // lib.neo.mkSystemdUnits ["neo-docker-updater"]
            // lib.neo.mkSystemdTimers ["neo-docker-updater"]
            // lib.neo.mkAppdata (lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata).dockerHistoryDir
            // lib.neo.mkServiceMeta {
              category = "Core";
              icon = ./icon.svg;
              tagline = "Keeps your apps up to date";
              description = ''
                Checks on a schedule whether new versions of your apps are available, downloads them and restarts the app.
                You do not have to update apps by hand.
              '';
              notes = ''
                Pulls newer images for every enabled service's containers (from the central containers registry) and restarts the matching docker-* units when an image changed.
                Shared images (e.g. one redis tag used by several services) are pulled once; every container using it is restarted.
                A declarative alternative to watchtower without an extra container. Image tags are set per container under each service's containers setting.
              '';
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Docker auto-updater configuration";
      };
    };
}
