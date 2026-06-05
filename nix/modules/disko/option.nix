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
    with lib; {
      options.neo.disko = mkOption {
        type = types.submodule {
          options = {
            enabled = mkEnableOption "Disko declarative partitioning with ZFS root (single disk)";

            mainDisk = mkOption {
              type = types.str;
              default = "/dev/vda";
              description = "Main disk (entire disk will be partitioned for EFI + ZFS root)";
            };

            additionalDisks = mkOption {
              type = types.attrsOf types.str;
              default = {};
              description = "Additional disks mapped to mountpoints (e.g. { \"/dev/sdb\" = \"/var/neo/DATA/Media\"; }). Each gets its own ZFS pool with snapshots disabled.";
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
