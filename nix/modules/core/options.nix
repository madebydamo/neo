# Global neo options (volumes, uid, gid, ssh, timezone, hostname, hashedLinuxPassword, nix build limits under core).
{...}: {
  flake.modules.nixos.core-options = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      ui = lib.neo.ui;
      # Collapsible sections on the System page. Ranks order the sections
      # after General; option ranks still order fields inside each section.
      group = {
        ssh = ui.mkGroup {
          id = "ssh";
          label = "SSH";
          description = "Login keys for admin and homeserver, and the optional console password";
          icon = ui.icons.key;
          rank = 10;
        };
        git = ui.mkGroup {
          id = "git";
          label = "Git";
          description = "Machine git identity for commits, private plugins and Hermes";
          icon = ui.icons.git;
          rank = 20;
        };
        volumes = ui.mkGroup {
          id = "volumes";
          label = "Volumes";
          description = "Where Neo stores service data, media and documents";
          icon = ui.icons.folder;
          rank = 30;
        };
        nix = ui.mkGroup {
          id = "nix";
          label = "Nix";
          description = "Local build limits and the remote builder";
          icon = ui.icons.cpu;
          rank = 40;
        };
      };
      inGroup = g: ui.mkUi {group = g;};
      summarized = g:
        ui.mkUi {
          group = g;
          summary = true;
        };
    in {
      options.neo.core.ssh.authorizedKeys = mkOption {
        type = types.listOf types.str;
        default = [];
        description = ''
          SSH authorized keys for admin and homeserver (set in settings.toml; the QEMU VM module adds the development key separately).
          When non-empty, SSH disables password/keyboard-interactive auth and root login (keys only).
          When empty, password auth stays enabled so you are not locked out before adding a key.
        '';
        rank = 0;
        ui = inGroup group.ssh;
      };

      # Machine git identity (modules/core/git.nix): homeserver builds/fetches
      # with it (private plugins), Hermes commits and clones with it.
      options.neo.core.git.userName = mkOption {
        type = types.str;
        default = "Neo";
        description = "Git user.name for commits on this machine (config repo, Hermes)";
        rank = 0;
        ui = summarized group.git;
      };

      options.neo.core.git.userEmail = mkOption {
        type = types.str;
        default = "neo@${config.neo.core.hostname}";
        description = "Git user.email for commits on this machine (config repo, Hermes)";
        rank = 10;
        ui = summarized group.git;
      };

      options.neo.core.git.tokens = lib.neo.mkSecretOption {
        type = types.attrsOf types.str;
        default = {};
        example = {"github.com" = "github_pat_…";};
        description = "HTTPS access tokens per git host (e.g. github.com = a fine-grained PAT with read access to your private plugin repos). Used for git over https and for Nix github:/gitlab: inputs. GitLab tokens for Nix need the PAT: prefix. SSH URLs (git+ssh://git@host/owner/repo) use the machine git SSH key instead";
        rank = 20;
        ui = inGroup group.git;
      };

      options.neo.core.git.knownHosts = mkOption {
        type = types.attrsOf types.str;
        default = {};
        description = "Extra git hosts reached over SSH with the machine git key, with their pinned SSH host public key (from ssh-keyscan -t ed25519 <host>, without the leading host name). github.com and gitlab.com are always included. Use host:port as the name for a non-standard SSH port (e.g. a Gitea at git.example.com:2222). Apply a new host before adding a plugin that lives on it";
        rank = 30;
        ui = inGroup group.git;
      };

      options.neo.core.git.useHostKey = mkOption {
        type = types.bool;
        default = false;
        description = "Use the machine's SSH host key as the git SSH key too (one identity for SSH, git and secret decryption, docs/design/secrets.md). Forges and SSH remotes then need the host public key instead of the separate git key. Off keeps the separate key at /var/lib/neo/git/id_ed25519";
        rank = 40;
        ui = inGroup group.git;
      };

      options.neo.core.git.users = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Local users that use the machine git identity (SSH key and tokens). Modules append themselves (core: homeserver; Hermes: hermes).";
        internal = true;
      };

      options.neo.core.plugins = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "List of plugin flake URLs (e.g. github:user/plugin or path:/path/to/plugin)";
        rank = 5;
        ui = lib.neo.ui.mkUi {
          widget = "pluginList";
          emptyHint = "No plugins yet. Add a flake URL (github:user/repo, git+file:/path, or path:/path).";
        };
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
        description = "Optional SHA-512 hash for the admin and homeserver users (web UI \"Hash password\" helper, or `mkpasswd -m sha-512` / `openssl passwd -6`). Empty (default) locks password login: SSH with authorizedKeys. Set this only if you want a console/sudo password. There is no shipped default password.";
        rank = 20;
        helper = lib.neo.helpers.mkpasswdSha512;
        ui = inGroup group.ssh;
      };

      options.neo.core.timeZone = mkOption {
        type = types.str;
        default = "Europe/Zurich";
        description = "System timezone";
        rank = 30;
      };

      options.neo.core.zramSwap.enabled = mkEnableOption "Compressed RAM swap (zram). Recommended on hosts with ≤8 GiB RAM or not much RAM left" {
        rank = 35;
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
        ui = inGroup group.nix;
      };

      options.neo.core.nix.cores = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "If set, configures nix.settings.cores (cores per Nix build job). Pair with maxJobs using the same recommendations: ≤ 8 GB RAM → 1; 8–16 GB RAM → leave it on default (null)";
        rank = 70;
        ui = inGroup group.nix;
      };

      # Local ranks inside remoteBuild.* only (parent rank places the group under core.nix).
      options.neo.core.nix.remoteBuild = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Offload Nix builds to a remote machine over SSH (uses the homeserver key by default)" {
                rank = 0;
                ui = summarized group.nix;
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
        ui = inGroup group.nix;
      };

      options.neo.core.volumes.root = mkOption {
        type = types.str;
        default = "/var/neo";
        description = "Root volume path";
        rank = 89;
        ui = summarized group.volumes;
      };

      options.neo.core.volumes.appdata = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/AppData";
        description = "AppData volume path";
        rank = 90;
        ui = inGroup group.volumes;
      };

      options.neo.core.volumes.data = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA";
        description = "Data volume path";
        rank = 100;
        ui = inGroup group.volumes;
      };

      options.neo.core.volumes.media = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Media";
        description = "Media volume path";
        rank = 110;
        ui = inGroup group.volumes;
      };

      options.neo.core.volumes.documents = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Documents";
        description = "Documents volume path";
        rank = 120;
        ui = inGroup group.volumes;
      };

      options.neo.migrations.applied = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Applied settings.toml migrations (managed by neo migrate command).";
        internal = true;
      };
    };
}
