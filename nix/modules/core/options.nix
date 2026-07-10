# Global neo options (volumes, uid, gid, ssh, timezone, hostname, hashedLinuxPassword, nix build limits under core).
{...}: {
  flake.modules.nixos.core-options = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.core.ssh.authorizedKeys = mkOption {
        type = types.listOf types.str;
        default = [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC9fFR8aERyyLjqI0aG08BXmSaMemGXK4WK8bLy7Nzmc development"
        ];
        description = "SSH authorized keys for root in VM";
        rank = 0;
      };

      options.neo.core.hostname = mkOption {
        type = types.str;
        default = "nixos";
        description = "System hostname";
        rank = 10;
      };

      options.neo.core.hashedLinuxPassword = mkOption {
        type = types.str;
        default = "";
        description = "Generate the hash using the web UI \"Hash password\" helper, or `mkpasswd -m sha-512` / `openssl passwd -6`. This sets the password for the user without exposing plaintext.";
        rank = 20;
        helper = lib.neo.helpers.mkpasswdSha512;
      };

      options.neo.core.timeZone = mkOption {
        type = types.str;
        default = "Europe/Zurich";
        description = "System timezone";
        rank = 30;
      };

      options.neo.core.uid = mkOption {
        type = types.int;
        default = 1000;
        description = "Global UID for services and containers";
        rank = 40;
      };

      options.neo.core.gid = mkOption {
        type = types.int;
        default = 1000;
        description = "Global GID for services and containers";
        rank = 50;
      };

      options.neo.core.nix.maxJobs = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "If set, configures nix.settings.max-jobs (maximum parallel Nix derivations). Recommendations: ≤ 8 GB RAM → 1; 8–16 GB RAM → leave it on default (null)";
        rank = 60;
      };

      options.neo.core.nix.cores = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "If set, configures nix.settings.cores (cores per Nix build job). Pair with maxJobs using the same recommendations: ≤ 8 GB RAM → 1; 8–16 GB RAM → leave it on default (null)";
        rank = 70;
      };

      options.neo.core.volumes.root = mkOption {
        type = types.str;
        default = "/var/neo";
        description = "Root volume path";
        rank = 80;
      };

      options.neo.core.volumes.appdata = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/AppData";
        description = "AppData volume path";
        rank = 90;
      };

      options.neo.core.volumes.data = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA";
        description = "Data volume path";
        rank = 100;
      };

      options.neo.core.volumes.media = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Media";
        description = "Media volume path";
        rank = 110;
      };

      options.neo.core.volumes.documents = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Documents";
        description = "Documents volume path";
        rank = 120;
      };

      options.neo.migrations.applied = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Applied settings.toml migrations (managed by neo migrate command).";
        internal = true;
      };
    };
}
