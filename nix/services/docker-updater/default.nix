# Docker auto-updater implementation: scheduled pulls + restarts using the containers registry from lib/containers.nix.
#
# Images are often shared across services (e.g. redis:7 for paperless + activepieces).
# Pull once per unique image; if the image ID changes, restart every container that uses it.
# Every run appends JSON + log under this service's appdata (updater/docker/).
# last.json is retargeted to an in-progress stub at start, then rewritten when
# the run finishes (or on SIGTERM).
#
# Image retention (docs/design/docker-image-retention.md): each run first
# deletes images outside the keep set (`neo docker-ledger gc`), and records in
# the image ledger which image ran under which reference when.
# neo-docker-rollback returns references to an older version from it (Hermes
# superviseUpdates, the web UI after a restore).
#
# The run holds the system lock shared, so it never overlaps an activation.
# Each restart holds the unit's neo operation lock (cli/src/utils/locks/,
# /run/neo/locks/unit_<unit>.lock): it waits for a UI start/stop/pull and skips a
# unit whose service is being restored or cleared (its units stay stopped; the
# pulled image is used when the restore starts it again).
{self, ...}: {
  flake.modules.nixos.docker-updater = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services."docker-updater";
      containers = neo.getAllContainers config;
      neoPkg = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
      updaterPaths = lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata;
      dockerHistoryDir = cfg.appdata;
      # Root dataset, never snapshotted: a data restore must not bring back an
      # older ledger (GC would forget images pulled after the snapshot).
      # Also LEDGER_PATH in cli/src/utils/docker_ledger.rs (the web UI reads it).
      ledger = "/var/lib/neo/docker-images.json";

      # Group by image so a single pull can restart all consumers of that image.
      containersByImage = groupBy (c: c.image) containers;

      # One entry per image (update.sh): a single pull restarts all its consumers.
      plan = pkgs.writeText "neo-docker-update-plan.json" (builtins.toJSON (map (image: let
        cs = containersByImage.${image};
      in {
        inherit image;
        consumers = concatMapStringsSep ", " (c: "${c.container} (${c.service})") cs;
        units = map (c: "docker-${c.container}") cs;
        restarts =
          map (c: {
            unit = "docker-${c.container}";
            inherit (c) service;
          })
          cs;
      }) (attrNames containersByImage)));

      # Every declared container image, also those outside the plan: never deleted.
      keepRefs = pkgs.writeText "neo-docker-keep-refs.json" (builtins.toJSON (unique (
        mapAttrsToList (_: c: c.image) config.virtualisation.oci-containers.containers
      )));

      updateScript = pkgs.writeShellApplication {
        name = "neo-docker-update";
        runtimeInputs = [pkgs.coreutils pkgs.docker pkgs.jq pkgs.systemd pkgs.util-linux];
        runtimeEnv = {
          NEO_HISTORY_DIR = dockerHistoryDir;
          NEO_PLAN = plan;
          NEO_LOCK_DIR = "/run/neo/locks";
          NEO_CLI = "${neoPkg}/bin/neo";
          NEO_LEDGER = ledger;
          NEO_KEEP_REFS = keepRefs;
          NEO_GC_OLDER_THAN = toString cfg.garbageCollectOlderThan;
        };
        text = builtins.readFile ./update.sh;
      };

      rollbackScript = pkgs.writeShellApplication {
        name = "neo-docker-rollback";
        runtimeInputs = [pkgs.coreutils pkgs.docker pkgs.jq pkgs.systemd pkgs.util-linux];
        runtimeEnv = {
          NEO_CLI = "${neoPkg}/bin/neo";
          NEO_PLAN = plan;
          NEO_LEDGER = ledger;
          NEO_HISTORY_DIR = dockerHistoryDir;
          NEO_LOCK_DIR = "/run/neo/locks";
          NEO_GC_OLDER_THAN = toString cfg.garbageCollectOlderThan;
        };
        text = builtins.readFile ./rollback.sh;
      };
    in {
      config = mkIf cfg.enabled {
        environment.systemPackages = [rollbackScript];

        # Web UI: "return the app to the version it ran then" after a restore.
        security.sudo.extraRules = lib.neo.mkSudoExtraRules {
          users = ["homeserver"];
          commands = [
            {
              package = rollbackScript;
              name = "neo-docker-rollback";
            }
          ];
        };

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
