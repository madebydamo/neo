# Disko options for declarative disk setup.
# mainDisk: 1M BIOS boot partition + 1G ESP (/boot) + ZFS pool (root + Neo data),
# bootable on both UEFI and BIOS firmware (see boot-install.sh).
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
            enabled = mkEnableOption "letting Neo partition and format the disks (used when installing)" {};

            mainDisk = mkOption {
              type = types.str;
              default = "/dev/vda";
              description = "The disk Neo installs onto. Everything on it is erased at install. Prefer the stable by-id name of the disk; short names like sda can change between boots.";
            };

            additionalDisks = mkOption {
              type = types.attrsOf types.str;
              default = {};
              description = "Additional disks mapped to mountpoints (e.g. { \"/dev/sdb\" = \"/var/neo/DATA/Media\"; } or { \"/dev/mmcblk0\" = \"/var/neo\"; } to back volumes.root with its own pool for easy swap/backup). Each gets an independent ZFS pool (snapshots disabled). Datasets use canmount=noauto and fstab nofail so only systemd mounts them (no zfs-mount race) and a missing/slow disk does not enter emergency mode.";
            };

            # Hybrid boot: a 1M BIOS boot partition before the ESP and GRUB in
            # the MBR, next to the UEFI install. Only takes effect when the
            # disk is partitioned; existing disks without the partition stay
            # UEFI-only (boot-install.sh skips the BIOS install).
            biosBoot = lib.mkOption {
              type = types.bool;
              default = true;
              internal = true;
              description = "Partition the main disk so it also boots on BIOS (legacy) firmware.";
            };

            bootEntry = mkOption {
              type = types.bool;
              default = true;
              rank = 90;
              description = "Add this server to the firmware boot menu, first in line, when it is missing. Some hosting providers otherwise boot an empty network or shell screen instead of the disk.";
              ui = lib.neo.ui.mkUi {
                label = "Add a firmware boot entry";
                group = lib.neo.ui.groups.advanced;
              };
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
