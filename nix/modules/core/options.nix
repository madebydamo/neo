# Global neo options (volumes, uid, gid, ssh, timezone).
{...}: {
  flake.modules.nixos.options = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.volumes.root = mkOption {
        type = types.str;
        default = "/var/neo";
        description = lib.mdDoc "Root volume path";
      };

      options.neo.volumes.appdata = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA/AppData";
        description = lib.mdDoc "AppData volume path";
      };

      options.neo.volumes.data = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA";
        description = lib.mdDoc "Data volume path";
      };

      options.neo.volumes.media = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA/Media";
        description = lib.mdDoc "Media volume path";
      };

      options.neo.volumes.documents = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA/Documents";
        description = lib.mdDoc "Documents volume path";
      };

      options.neo.uid = mkOption {
        type = types.int;
        default = 1000;
        description = lib.mdDoc "Global UID for services and containers";
      };

      options.neo.gid = mkOption {
        type = types.int;
        default = 1000;
        description = lib.mdDoc "Global GID for services and containers";
      };

      options.neo.ssh.authorizedKeys = mkOption {
        type = types.listOf types.str;
        default = [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC9fFR8aERyyLjqI0aG08BXmSaMemGXK4WK8bLy7Nzmc development"
        ];
        description = lib.mdDoc "SSH authorized keys for root in VM";
      };

      options.neo.timeZone = mkOption {
        type = types.str;
        default = "Europe/Zurich";
        description = lib.mdDoc "System timezone";
      };

      options.neo.device = mkOption {
        type = types.submodule (
          {...}: {
            options = {
              hostname = mkOption {
                type = types.str;
                default = "nixos";
                description = lib.mdDoc "System hostname";
              };
              grubDevice = mkOption {
                type = types.str;
                default = "/dev/vda";
                description = lib.mdDoc "GRUB installation device";
              };
              rootFsDevice = mkOption {
                type = types.str;
                default = "/dev/vda1";
                description = lib.mdDoc "Root filesystem device";
              };
              rootFsType = mkOption {
                type = types.str;
                default = "ext4";
                description = lib.mdDoc "Root filesystem type";
              };
            };
          }
        );
        default = {};
        description = lib.mdDoc "Device/machine specific settings from settings.toml";
      };
    };
}
