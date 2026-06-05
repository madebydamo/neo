# Global neo options (volumes, uid, gid, ssh, timezone, hostname, hashedLinuxPassword under core).
{...}: {
  flake.modules.nixos.core-options = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.core.volumes.root = mkOption {
        type = types.str;
        default = "/var/neo";
        description = "Root volume path";
      };

      options.neo.core.volumes.appdata = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/AppData";
        description = "AppData volume path";
      };

      options.neo.core.volumes.data = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA";
        description = "Data volume path";
      };

      options.neo.core.volumes.media = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Media";
        description = "Media volume path";
      };

      options.neo.core.volumes.documents = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Documents";
        description = "Documents volume path";
      };

      options.neo.core.uid = mkOption {
        type = types.int;
        default = 1000;
        description = "Global UID for services and containers";
      };

      options.neo.core.gid = mkOption {
        type = types.int;
        default = 1000;
        description = "Global GID for services and containers";
      };

      options.neo.core.ssh.authorizedKeys = mkOption {
        type = types.listOf types.str;
        default = [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC9fFR8aERyyLjqI0aG08BXmSaMemGXK4WK8bLy7Nzmc development"
        ];
        description = "SSH authorized keys for root in VM";
      };

      options.neo.core.timeZone = mkOption {
        type = types.str;
        default = "Europe/Zurich";
        description = "System timezone";
      };

      options.neo.core.hostname = mkOption {
        type = types.str;
        default = "nixos";
        description = "System hostname";
      };

      options.neo.core.hashedLinuxPassword = mkOption {
        type = types.str;
        default = "";
        description = "Generate the hash using `mkpasswd -m sha-512` (from pkgs.mkpasswd) or `openssl passwd -6`. This sets the password for the user without exposing plaintext.";
      };
      options.neo.migrations.applied = mkOption {
        type = types.listOf types.str;
        default = [];
        description = lib.mdDoc "Applied settings.toml migrations (managed by neo migrate command).";
        internal = true;
      };
    };
}
