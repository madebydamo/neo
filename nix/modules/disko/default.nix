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
                options."com.sun:auto-snapshot" = "false";
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

    services.zfs.autoSnapshot = lib.mkIf cfg.enabled {
      enable = true;
      flags = "-k -p --utc";
      frequent = 4;
      hourly = 12;
      daily = 7;
      weekly = 4;
      monthly = 3;
    };

    #TODO
    system.activationScripts.create-volumes = lib.mkIf cfg.enabled (lib.mkForce (
      lib.concatStringsSep "\n" (
        lib.map
        (dir:
          lib.neo.mkActivationScriptForDir config {
            dirPath = "${dir}";
          })
        [
          config.neo.core.volumes.data
          config.neo.core.volumes.appdata
          config.neo.core.volumes.media
          config.neo.core.volumes.documents
        ]
      )
    ));
  };
}
