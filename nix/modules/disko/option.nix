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
          };
        };
        default = {};
        description = "Disko configuration for homeserver storage";
      };
    };
}
