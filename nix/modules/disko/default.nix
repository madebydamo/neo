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
    # The main pool's Neo data dataset is mounted in the initrd (see fileSystems).
    mountDataInInitrd = cfg.enabled && !(builtins.elem volumesRoot additionalMountpoints);
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
              partitions =
                lib.optionalAttrs cfg.biosBoot {
                  # GRUB's core image for BIOS firmware (boot-install.sh).
                  bios = {
                    size = "1M";
                    type = "EF02";
                  };
                }
                // {
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

    # disko adds the EF02 disk to grub.devices; NixOS would then run a BIOS
    # grub-install that fails on disks partitioned before the BIOS boot
    # partition existed. boot-install.sh does it instead, only when present.
    boot.loader.grub = lib.mkIf cfg.enabled {
      devices = lib.mkForce ["nodev"];
      extraInstallCommands = let
        flag = b:
          if b
          then "1"
          else "0";
        efiArch = pkgs.stdenv.hostPlatform.efiArch;
        loader = "\\EFI\\BOOT\\BOOT${lib.toUpper efiArch}.EFI";
        # The EFI Shell runs startup.nsh from the first filesystem it maps:
        # boot the disk from there too.
        startupNsh = pkgs.writeText "startup.nsh" (lib.replaceStrings ["\n"] ["\r\n"] ''
          @echo -off
          for %d in fs0 fs1 fs2 fs3 fs4 fs5 fs6 fs7
            if exist %d:${loader} then
              %d:${loader}
            endif
          endfor
        '');
        install = pkgs.writeShellApplication {
          name = "neo-boot-install";
          # Every step logs and goes on; see boot-install.sh.
          bashOptions = ["nounset"];
          runtimeInputs = with pkgs; [coreutils gnugrep gnused util-linux diffutils efibootmgr];
          text = builtins.readFile ./boot-install.sh;
        };
      in ''
        NEO_ESP=${lib.escapeShellArg config.boot.loader.efi.efiSysMountPoint} \
        NEO_BIOS=${flag cfg.biosBoot} \
        NEO_BOOT_ENTRY=${flag cfg.bootEntry} \
        NEO_GRUB=${pkgs.grub2} \
        NEO_EFI_LOADER=${lib.escapeShellArg loader} \
        NEO_STARTUP_NSH=${startupNsh} \
          ${lib.getExe install} || true
      '';
    };

    boot.supportedFilesystems = lib.mkIf cfg.enabled ["zfs"];
    boot.zfs = lib.mkIf cfg.enabled {
      forceImportRoot = lib.mkDefault false;
      forceImportAll = lib.mkDefault false;
    };

    environment.systemPackages = lib.mkIf cfg.enabled [pkgs.zfs];

    fileSystems = lib.mkMerge [
      # Secondary ZFS mounts are data disks: never block boot if absent/slow.
      # Disko already emits nofail via mountOptions; pin neededForBoot as well.
      (lib.mkIf (cfg.enabled && cfg.additionalDisks != {}) (
        lib.listToAttrs (map (mp: {
            name = mp;
            value = {
              neededForBoot = lib.mkDefault false;
              options = lib.mkAfter ["nofail"];
            };
          })
          additionalMountpoints)
      ))
      # The Neo data dataset lives in the main pool (imported in the initrd for
      # /): mount it before activation, which writes into appdata (Hermes
      # .env / config.yaml, secret copies such as SWAG snippets). Mounted later,
      # those files land under the mountpoint and the dataset hides them.
      # neo-zfs-restore still runs first (before sysroot.mount).
      (lib.mkIf mountDataInInitrd {
        ${volumesRoot}.neededForBoot = true;
      })
    ];

    # The disko VM (just launch) builds fileSystems from virtualisation.fileSystems.
    virtualisation.vmVariantWithDisko = lib.mkIf mountDataInInitrd {
      virtualisation.fileSystems.${volumesRoot}.neededForBoot = true;
    };

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
      script =
        lib.concatMapStringsSep "\n" (mp: let
          ds = additionalDataset mp;
        in ''
          if zfs list -H -o name ${lib.escapeShellArg ds} >/dev/null 2>&1; then
            cur=$(zfs get -H -o value canmount ${lib.escapeShellArg ds} || true)
            if [ "$cur" != "noauto" ]; then
              zfs set canmount=noauto ${lib.escapeShellArg ds} || true
            fi
          fi
        '')
        additionalMountpoints;
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
