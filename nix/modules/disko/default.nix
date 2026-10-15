{inputs, ...}: {
  flake.modules.nixos.disko = {
    config,
    lib,
    pkgs,
    utils,
    ...
  }: let
    cfg = config.neo.disko;
    sanitize = n: lib.replaceStrings ["/" "-" "." " "] ["" "" "" ""] (baseNameOf n);
    volumesRoot = config.neo.core.volumes.root;
    # Restore of the whole Neo data dataset runs in the systemd initrd
    # (see zfs-restore.sh). The system itself (root, /nix) is never snapshotted.
    dataRestore = cfg.enabled && config.boot.initrd.systemd.enable;
    dataDataset = "${cfg.poolName}/neo";
    additionalMountpoints = lib.attrValues cfg.additionalDisks;
    additionalPoolName = mp: "zpool-${sanitize mp}";
    additionalDataset = mp: "${additionalPoolName mp}/data";
    # systemd unit name for a path mount (e.g. /mnt/foo → mnt-foo.mount)
    additionalMountUnit = mp: "${utils.escapeSystemdPath mp}.mount";
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
          pool = additionalPoolName mp;
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

    # Secondary ZFS mounts are data disks: never block boot if absent/slow.
    # Disko already emits nofail via mountOptions; pin neededForBoot as well.
    fileSystems = lib.mkIf (cfg.enabled && cfg.additionalDisks != {}) (
      lib.listToAttrs (map (mp: {
          name = mp;
          value = {
            neededForBoot = lib.mkDefault false;
            options = lib.mkAfter ["nofail"];
          };
        })
        additionalMountpoints)
    );

    # Disko applies dataset options at create/update time. Existing
    # additional-disk datasets may still have canmount=on until then. Set
    # canmount=noauto after the pool is imported and before both zfs-mount
    # and the systemd path mounts, so neither side races on first activate.
    systemd.services.neo-additional-disks-zfs-prep = lib.mkIf (cfg.enabled && cfg.additionalDisks != {}) {
      description = "Neo: canmount=noauto on additional ZFS data datasets";
      wantedBy = ["zfs-mount.service" "local-fs.target" "multi-user.target"];
      after =
        ["zfs-import.target"]
        ++ map (mp: "zfs-import-${additionalPoolName mp}.service") additionalMountpoints;
      before =
        ["zfs-mount.service" "local-fs.target"]
        ++ map additionalMountUnit additionalMountpoints;
      wants = map (mp: "zfs-import-${additionalPoolName mp}.service") additionalMountpoints;
      unitConfig.DefaultDependencies = "no";
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
      path = [pkgs.zfs pkgs.coreutils];
      script = lib.concatMapStringsSep "\n" (mp: let
        ds = additionalDataset mp;
      in ''
        if zfs list -H -o name ${lib.escapeShellArg ds} >/dev/null 2>&1; then
          cur=$(zfs get -H -o value canmount ${lib.escapeShellArg ds} || true)
          if [ "$cur" != "noauto" ]; then
            zfs set canmount=noauto ${lib.escapeShellArg ds} || true
          fi
        fi
      '') additionalMountpoints;
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
