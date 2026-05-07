# Global neo options (volumes, uid, gid, ssh, timezone, device, users).
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
        description = "Root volume path";
      };

      options.neo.volumes.appdata = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA/AppData";
        description = "AppData volume path";
      };

      options.neo.volumes.data = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA";
        description = "Data volume path";
      };

      options.neo.volumes.media = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA/Media";
        description = "Media volume path";
      };

      options.neo.volumes.documents = mkOption {
        type = types.str;
        default = "${config.neo.volumes.root}/DATA/Documents";
        description = "Documents volume path";
      };

      options.neo.uid = mkOption {
        type = types.int;
        default = 1000;
        description = "Global UID for services and containers";
      };

      options.neo.gid = mkOption {
        type = types.int;
        default = 1000;
        description = "Global GID for services and containers";
      };

      options.neo.ssh.authorizedKeys = mkOption {
        type = types.listOf types.str;
        default = [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC9fFR8aERyyLjqI0aG08BXmSaMemGXK4WK8bLy7Nzmc development"
        ];
        description = "SSH authorized keys for root in VM";
      };

      options.neo.timeZone = mkOption {
        type = types.str;
        default = "Europe/Zurich";
        description = "System timezone";
      };

      options.neo.device = mkOption {
        type = types.submodule (
          {...}: {
            options = {
              hostname = mkOption {
                type = types.str;
                default = "nixos";
                description = "System hostname";
              };
            };
          }
        );
        default = {};
        description = "Device/machine specific settings from settings.toml";
      };

      options.neo.users = mkOption {
        type = types.submodule (
          {...}: {
            options = {
              hashedPassword = mkOption {
                type = types.str;
                default = "";
                description = "Generate the hash using `mkpasswd -m sha-512` (from pkgs.mkpasswd) or `openssl passwd -6`. This sets the password for the user without exposing plaintext.";
              };
            };
          }
        );
        default = {};
        description = "User settings";
      };
    };
}
