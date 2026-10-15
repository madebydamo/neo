{inputs, ...}: {
  flake.modules.nixos.disko = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.disko;
    sanitize = n: lib.replaceStrings ["/" "-" "." " "] ["" "" "" ""] (baseNameOf n);
    volumesRoot = config.neo.core.volumes.root;
    # Restore of the whole Neo data dataset runs in the systemd initrd
    # (see zfs-restore.sh). The system itself (root, /nix) is never snapshotted.
    dataRestore = cfg.enabled && config.boot.initrd.systemd.enable;
    dataDataset = "${cfg.poolName}/neo";
  in {
    imports = [inputs.disko.nixosModules.disko];
    disko.devices = lib.mkIf cfg.enabled {
      disk =
        {
          main = {
            type = "disk";
            device = cfg.mainDisk;
            content = {
              type = "gpt";
              partitions = {
                ESP = {
                  size = "1G";
                  type = "EF00";
                  content = {
                    type = "filesystem";
                    format = "vfat";
                    mountpoint = "/boot";
                    mountOptions = ["umask=0077"];
                  };
                };
                zfs = {
                  size = "100%";
                  content = {
                    type = "zfs";
                    pool = cfg.poolName;
                  };
                };
              };
            };
          };
        }
        // lib.mapAttrs' (disk: mp: {
          name = "disk-${sanitize disk}";
          value = {
            type = "disk";
            device = disk;
            content = {
              type = "gpt";
              partitions = {
                zfs = {
                  size = "100%";
                  content = {
                    type = "zfs";
                    pool = "zpool-${sanitize mp}";
                  };
                };
              };
            };
          };
        })
        cfg.additionalDisks;

      zpool =
        {
          ${cfg.poolName} = {
            type = "zpool";
            mountpoint = null;
            rootFsOptions = {
              compression = "zstd";
              "com.sun:auto-snapshot" = "false";
            };
            datasets = {
              root = {
                type = "zfs_fs";
                mountpoint = "/";
                options."com.sun:auto-snapshot" = "false";
              };
              neo = {
                type = "zfs_fs";
                mountpoint = volumesRoot;
                options."com.sun:auto-snapshot" = "true";
              };
            };
          };
        }
        // lib.mapAttrs' (disk: mp: let
          pool = "zpool-${sanitize mp}";
        in {
          name = pool;
          value = {
            type = "zpool";
            mountpoint = null;
            rootFsOptions = {
              compression = "zstd";
              "com.sun:auto-snapshot" = "false";
            };
            datasets = {
              data = {
                type = "zfs_fs";
                mountpoint = mp;
                # Single mounter: systemd fileSystems (disko + zfsutil).
                # canmount=noauto is skipped by `zfs mount -a` (zfs-mount.service),
                # so only the fstab unit mounts the dataset — no dual-mount race.
                # nofail keeps a missing/slow secondary disk out of emergency mode.
                mountOptions = ["defaults" "nofail"];
                options = {
                  "com.sun:auto-snapshot" = "false";
                  canmount = "noauto";
                };
              };
            };
          };
        })
        cfg.additionalDisks;
    };

    networking.hostId = "4d681778";

    boot.supportedFilesystems = lib.mkIf cfg.enabled ["zfs"];
    boot.zfs = lib.mkIf cfg.enabled {
      forceImportRoot = lib.mkDefault false;
      forceImportAll = lib.mkDefault false;
    };

    environment.systemPackages = lib.mkIf cfg.enabled [pkgs.zfs];

    # Disko applies dataset options at create time. Existing additional-disk
    # datasets may still have canmount=on until updated. Set canmount=noauto
    # before zfs-mount (property change is safe while mounted) so one activate
    # stops the dual-mount race without a manual zfs set.
    systemd.services.neo-additional-disks-zfs-prep = lib.mkIf (cfg.enabled && cfg.additionalDisks != {}) {
      description = "Neo: canmount=noauto on additional ZFS data datasets";
      wantedBy = ["zfs-mount.service" "multi-user.target"];
      after = ["zfs-import.target"];
      before = ["zfs-mount.service"];
      unitConfig.DefaultDependencies = "no";
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
      path = [pkgs.zfs pkgs.coreutils];
      script = lib.concatMapStringsSep "\n" (mp: let
        ds = "zpool-${sanitize mp}/data";
      in ''
        if zfs list -H -o name ${lib.escapeShellArg ds} >/dev/null 2>&1; then
          cur=$(zfs get -H -o value canmount ${lib.escapeShellArg ds} || true)
          if [ "$cur" != "noauto" ]; then
            zfs set canmount=noauto ${lib.escapeShellArg ds} || true
          fi
        fi
      '') (lib.attrValues cfg.additionalDisks);
    };

    boot.initrd.systemd.services = lib.mkIf dataRestore {
      neo-zfs-restore = {
        description = "Neo: restore Neo data to a ZFS snapshot (scheduled from the web UI)";
        wantedBy = ["initrd.target"];
        requires = ["zfs-import-${cfg.poolName}.service"];
        after = ["zfs-import-${cfg.poolName}.service"];
        before = ["sysroot.mount" "initrd-root-fs.target" "initrd-fs.target"];
        unitConfig.DefaultDependencies = "no";
        serviceConfig.Type = "oneshot";
        environment.NEO_POOL = cfg.poolName;
        script = builtins.readFile ./zfs-restore.sh;
      };
    };

    # neo-web: the full data restore is offered only where the initrd hook exists.
    systemd.services.neo-web.environment = lib.mkIf (dataRestore && config.neo.services.neo.enabled) {
      NEO_ZFS_RESTORE_POOL = cfg.poolName;
      NEO_ZFS_RESTORE_DATASET = dataDataset;
    };

    # zfstools prunes with `zfs destroy -d`: a snapshot pinned in the web UI
    # (`zfs hold neo-pin`) is only marked for deferred destroy and survives.
    services.zfs.autoSnapshot = lib.mkIf (cfg.enabled && cfg.autoSnapshot.enabled) {
      enable = true;
      flags = "-k -p --utc";
      inherit (cfg.autoSnapshot) frequent hourly daily weekly monthly;
    };

    # Disko owns the root dataset; ensure the Neo data volume mountpoints only.
    system.activationScripts.create-volumes = lib.mkIf cfg.enabled (lib.mkForce (
      lib.neo.mkEnsureDirs config [
        config.neo.core.volumes.data
        config.neo.core.volumes.appdata
        config.neo.core.volumes.media
        config.neo.core.volumes.documents
      ]
    ));
  };
}
