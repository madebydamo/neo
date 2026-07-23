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
        default = [];
        description = ''
          SSH authorized keys for admin and homeserver (set in settings.toml; the QEMU VM module adds the development key separately).
          When non-empty, SSH disables password/keyboard-interactive auth and root login (keys only).
          When empty, password auth stays enabled so you are not locked out before adding a key.
        '';
        rank = 0;
      };

      options.neo.core.plugins = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "List of plugin flake URLs (e.g. github:user/plugin or path:/path/to/plugin)";
        rank = 5;
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

      # Local ranks inside remoteBuild.* only (parent rank places the group under core.nix).
      options.neo.core.nix.remoteBuild = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Offload Nix builds to a remote machine over SSH (uses the homeserver key by default)" {
                rank = 0;
              };

              maxJobs = mkOption {
                type = types.int;
                default = 8;
                description = "Maximum parallel build jobs on the remote machine (roughly its usable cores)";
                rank = 50;
              };

              speedFactor = mkOption {
                type = types.int;
                default = 1;
                description = "Relative speed of the remote builder (higher = preferred when multiple builders exist)";
                rank = 60;
              };

              system = mkOption {
                type = types.str;
                default = "x86_64-linux";
                description = "Nix system type of the remote builder (e.g. x86_64-linux, aarch64-linux)";
                rank = 70;
              };

              supportedFeatures = mkOption {
                type = types.listOf types.str;
                default = ["nixos-test" "benchmark" "big-parallel" "kvm"];
                description = "Nix features the remote builder supports";
                rank = 80;
              };

              publicHostKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Optional base64-encoded SSH host public key for the builder (avoids interactive host-key prompts). From ssh-keyscan output, base64-encode the key type and key fields only";
                rank = 90;
              };
            }
            // lib.neo.mkSshConnectionOptions {
              # After enabled (0): host/user/sshKey/extra = 10–40, then maxJobs…
              rankBase = 10;
              hostDescription = "Remote build machine hostname or IP (or SSH Host alias)";
              userDescription = "SSH user on the remote build machine (must be in nix.trusted-users there)";
              sshKeyDescription = "SSH private key for remote builds. Defaults to the auto-generated homeserver key; override only if needed. The nix-daemon (root) uses this key.";
              extraOptionsDescription = "Additional SSH options for the nix-daemon SSH client (passed via NIX_SSHOPTS), e.g. -o StrictHostKeyChecking=accept-new";
            };
        };
        default = {};
        description = "Remote Nix builder: when enabled, heavy builds (including nixosConfigurations) are delegated over SSH so a weak local machine only evaluates and fetches results";
        rank = 81;
      };

      options.neo.core.volumes.root = mkOption {
        type = types.str;
        default = "/var/neo";
        description = "Root volume path";
        rank = 89;
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
