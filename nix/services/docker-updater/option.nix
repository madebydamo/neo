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
              # Scheduled image pulls + restarts of the declared containers.
              enabled = mkEnableOption "Automatically update apps: download new versions and restart the apps that changed" {
                rank = 0;
                default = true;
              };
              # systemd OnCalendar expression.
              schedule = mkOption {
                type = types.str;
                # An hour after the system updater (daily 04:00), so the two do not
                # start together; the system lock also makes them wait for each other.
                default = "Sun *-*-* 05:00:00";
                description = "When to check for and install app updates (default: Sundays at 5 AM, after the system update). Accepts daily, weekly or a systemd calendar time such as Sun 05:00.";
                rank = 10;
                ui = lib.neo.ui.mkUi {
                  label = "Update time";
                  visibleWhen = "enabled";
                };
              };
              # `neo docker-ledger gc --older-than` at the start of each run: replaced
              # images older than this, and every image no container uses, are deleted
              # (docs/design/docker-image-retention.md). Units s/m/h/d/w; null skips GC.
              garbageCollectOlderThan = mkOption {
                type = types.nullOr (types.strMatching "[0-9]+[smhdw]");
                default = "30d";
                description = "Delete old app versions after this long (e.g. 30d) to free disk space. Older versions can no longer be rolled back to without downloading them again.";
                rank = 20;
                ui = lib.neo.ui.mkUi {
                  label = "Delete old versions after";
                  visibleWhen = "enabled";
                };
              };
            }
            // lib.neo.mkSystemdUnits ["neo-docker-updater"]
            // lib.neo.mkSystemdTimers ["neo-docker-updater"]
            // lib.neo.mkAppdata (lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata).dockerHistoryDir
            // lib.neo.mkServiceMeta {
              category = "Core";
              license = {
                spdx = "MIT";
                url = "https://github.com/madebydamo/neo/blob/master/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Keeps your apps up to date";
              description = ''
                Checks on a schedule whether new versions of your apps are available, downloads them and restarts the app.
                You do not have to update apps by hand.
              '';
              notes = ''
                Pulls newer images for every enabled service's containers (from the central containers registry) and restarts the matching docker-* units when an image changed.
                Shared images (e.g. one redis tag used by several services) are pulled once; every container using it is restarted.
                Each run first deletes app versions replaced longer ago than Delete old versions after, and every image no container uses (including ones you built or pulled by hand and have not started).
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
