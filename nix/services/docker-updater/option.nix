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
            // lib.neo.mkAppdata (lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata).dockerHistoryDir
            // lib.neo.mkServiceMeta {
              category = "Core";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/watchtower.svg";
              description = ''
                Periodically pulls newer images for every enabled service's declared Docker containers (from the central containers registry) and restarts the corresponding docker-* units when a newer image is downloaded. Shared images (e.g. the same redis:tag used by several services) are pulled once; every consumer container is restarted when that image ID changes. Provides a declarative, UI-integrated alternative to watchtower without extra runtime containers. Image tags are configurable per-container under each service's "containers" setting (applied on activate).
              '';
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Docker auto-updater configuration";
      };
    };
}
