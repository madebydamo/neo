# Docker auto-updater implementation: scheduled pulls + restarts using the containers registry from lib/containers.nix.
#
# Images are often shared across services (e.g. redis:7 for paperless + activepieces).
# Pull once per unique image; if the image ID changes, restart every container that uses it.
# Every run appends JSON + log under this service's appdata (updater/docker/).
# last.json is retargeted to an in-progress stub at start, then rewritten when
# the run finishes (or on SIGTERM). When Hermes superviseUpdates is on, also
# tag the previous image so Hermes can roll back a broken pull.
#
# The run holds the system lock shared, so it never overlaps an activation.
# Each restart holds the unit's neo operation lock (cli/src/utils/locks/,
# /run/neo/locks/unit_<unit>.lock): it waits for a UI start/stop/pull and skips a
# unit whose service is being restored or cleared (its units stay stopped; the
# pulled image is used when the restore starts it again).
{...}: {
  flake.modules.nixos.docker-updater = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services."docker-updater";
      containers = neo.getAllContainers config;
      supervise =
        (config.neo.services.hermes.enabled or false)
        && (config.neo.services.hermes.superviseUpdates or false);
      updaterPaths = lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata;
      dockerHistoryDir = cfg.appdata;
      dockerManifest = "${cfg.appdata}/last.json";

      # Group by image so a single pull can restart all consumers of that image.
      containersByImage = groupBy (c: c.image) containers;

      rollbackTagFor = image: "neo-rollback:${replaceStrings ["/" ":"] ["__" "__"] image}";

      # One entry per image (update.sh): a single pull restarts all its consumers.
      plan = pkgs.writeText "neo-docker-update-plan.json" (builtins.toJSON (map (image: let
        cs = containersByImage.${image};
      in {
        inherit image;
        consumers = concatMapStringsSep ", " (c: "${c.container} (${c.service})") cs;
        rollback_tag = rollbackTagFor image;
        units = map (c: "docker-${c.container}") cs;
        restarts =
          map (c: {
            unit = "docker-${c.container}";
            inherit (c) service;
          })
          cs;
      }) (attrNames containersByImage)));

      updateScript = pkgs.writeShellApplication {
        name = "neo-docker-update";
        runtimeInputs = [pkgs.coreutils pkgs.docker pkgs.jq pkgs.systemd pkgs.util-linux];
        runtimeEnv = {
          NEO_HISTORY_DIR = dockerHistoryDir;
          NEO_PLAN = plan;
          NEO_SUPERVISE = boolToString supervise;
          NEO_LOCK_DIR = "/run/neo/locks";
        };
        text = builtins.readFile ./update.sh;
      };

      rollbackScript = pkgs.writeShellApplication {
        name = "neo-docker-rollback";
        runtimeInputs = [pkgs.docker pkgs.jq pkgs.systemd];
        runtimeEnv.NEO_MANIFEST = dockerManifest;
        text = builtins.readFile ./rollback.sh;
      };
    in {
      config = mkIf cfg.enabled {
        environment.systemPackages = optional supervise rollbackScript;

        system.activationScripts.neo-docker-updater-history = lib.neo.mkUpdaterHistoryDirs config [updaterPaths.stateDir dockerHistoryDir];

        systemd.services.neo-docker-updater = {
          description = "Pull updated Docker images for neo services and restart containers";
          serviceConfig = {
            Type = "oneshot";
            # System scope shared for the whole run (cli/src/utils/locks/): an
            # activation (system exclusive) waits for it and it waits for one, so
            # an update never restarts apps while a switch stops and starts them.
            ExecStart = "${pkgs.util-linux}/bin/flock -s -w 3600 /run/neo/locks/system.lock ${getExe updateScript}";
            User = "root";
          };
        };

        systemd.timers.neo-docker-updater = {
          wantedBy = ["timers.target"];
          timerConfig = {
            OnCalendar = cfg.schedule;
            Persistent = true;
            RandomizedDelaySec = "30m";
            AccuracySec = "1h";
            Unit = "neo-docker-updater.service";
          };
        };
      };
    };
}
