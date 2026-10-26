# Disko options for declarative disk setup.
# Single mainDisk is fully ZFS (EFI boot partition + ZFS root partition).
# Root dataset (/) has no snapshots; neo dataset at neo.core.volumes.root has snapshots.
# Additional disks get independent ZFS pools (snapshots disabled).
{...}: {
  flake.modules.nixos.disko-options = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.disko = mkOption {
        type = types.submodule {
          options = {
            enabled = mkEnableOption "Disko declarative partitioning" {};

            mainDisk = mkOption {
              type = types.str;
              default = "/dev/vda";
              description = "Main disk device for OS (EFI partition + root fs according to rootFilesystem)";
            };

            additionalDisks = mkOption {
              type = types.attrsOf types.str;
              default = {};
              description = "Additional disks mapped to mountpoints (e.g. { \"/dev/sdb\" = \"/var/neo/DATA/Media\"; } or { \"/dev/mmcblk0\" = \"/var/neo\"; } to back volumes.root with its own pool for easy swap/backup). Each gets independent ZFS pool (snapshots disabled).";
            };

            poolName = mkOption {
              type = types.str;
              default = "zroot";
              description = "Name of the main ZFS pool";
            };

            # Retention of the automatic snapshots of the Neo data dataset
            # (services.zfs.autoSnapshot / zfstools). Manual snapshots taken in
            # the web UI are never pruned; pinned (held) ones are skipped.
            autoSnapshot = mkOption {
              type = types.submodule {
                options = let
                  keep = rank: default: what:
                    mkOption {
                      type = types.ints.unsigned;
                      inherit default rank;
                      description = "How many ${what} snapshots to keep. 0 turns this interval off and deletes its existing automatic snapshots.";
                      ui = lib.neo.ui.mkUi {visibleWhen = "enabled";};
                    };
                in {
                  enabled = mkEnableOption "automatic snapshots of all Neo data" {
                    default = true;
                    rank = 0;
                  };
                  frequent = keep 10 4 "15-minute";
                  hourly = keep 20 12 "hourly";
                  daily = keep 30 7 "daily";
                  weekly = keep 40 4 "weekly";
                  monthly = keep 50 3 "monthly";
                };
              };
              default = {};
              rank = 100;
              description = "Automatic snapshots of all Neo data and how many of each interval are kept. Older ones are deleted automatically; manual and pinned snapshots are never deleted.";
              ui = lib.neo.ui.mkUi {
                group = lib.neo.ui.mkGroup {
                  id = "snapshots";
                  label = "Automatic snapshots";
                  description = "How long automatic snapshots of all Neo data are kept";
                  icon = lib.neo.ui.icons.camera;
                  rank = 100;
                };
              };
            };
          };
        };
        default = {};
        description = "Disko configuration for homeserver storage";
      };
    };
}
