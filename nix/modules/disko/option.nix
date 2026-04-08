# Disko options for declarative disk setup.
# Single mainDisk is fully ZFS (EFI boot partition + ZFS root partition).
# Root dataset (/) has no snapshots; neo dataset at neo.volumes.root has snapshots.
# Additional disks get independent ZFS pools (snapshots disabled).
{...}: {
  flake.modules.nixos.options = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.disko = mkOption {
        type = types.submodule {
          options = {
            enabled = mkEnableOption (mdDoc "Disko declarative partitioning with ZFS root (single disk)");

            mainDisk = mkOption {
              type = types.str;
              default = "/dev/vda";
              description = mdDoc "Main disk (entire disk will be partitioned for EFI + ZFS root)";
            };

            additionalDisks = mkOption {
              type = types.attrsOf types.str;
              default = {};
              description = mdDoc "Additional disks mapped to mountpoints (e.g. { \"/dev/sdb\" = \"/var/neo/DATA/Media\"; }). Each gets its own ZFS pool with snapshots disabled.";
            };

            poolName = mkOption {
              type = types.str;
              default = "zroot";
              description = mdDoc "Name of the main ZFS pool";
            };
          };
        };
        default = {};
        description = mdDoc "Disko configuration for homeserver storage";
      };
    };
}
