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
          description = "How this server signs its changes and gets into your private plugin repositories";
          icon = ui.icons.git;
          rank = 20;
        };
        volumes = ui.mkGroup {
          id = "volumes";
          label = "Volumes";
          description = "Folders where apps keep their data, media and documents";
          icon = ui.icons.folder;
          rank = 30;
        };
        # Expert settings: user/group IDs, Nix build limits and remote
        # builders, git host key options.
        advanced = ui.mkGroup {
          id = "advanced";
          label = "Advanced";
          description = "Build limits, remote builders, user IDs and other expert settings";
          icon = ui.icons.wrench;
          rank = 90;
        };
      };
      inGroup = g: ui.mkUi {group = g;};
      summarized = g:
        ui.mkUi {
          group = g;
          summary = true;
        };
      labelled = g: label:
        ui.mkUi {
          inherit label;
          group = g;
        };
    in {
      # When non-empty, SSH disables password/keyboard-interactive auth and
      # root login (keys only). The QEMU VM module adds the development key separately.
      options.neo.core.ssh.authorizedKeys = mkOption {
        type = types.listOf types.str;
        default = [];
        description = ''
          Public SSH keys allowed to log in as admin and homeserver.
          Once you add a key, password login over SSH is turned off; while this is empty it stays on so you are not locked out.
        '';
        rank = 0;
        ui = labelled group.ssh "Authorized SSH keys";
      };

      # Login as homeserver only (docs/design/ssh-key-roles.md). Never a
      # recipient of secrets.sops.yaml, identity.sops.yaml or neo-backup.age,
      # and never in root's or admin's authorized_keys. Password login follows
      # authorizedKeys alone.
      options.neo.core.ssh.machineKeys = mkOption {
        type = types.listOf types.str;
        default = [];
        description = ''
          Public SSH keys of other computers, such as a server that builds updates here. They can log in and manage this server, but they can't open your backups or encrypted settings.
          Put the keys of people in Authorized SSH keys instead, so they can always restore.
        '';
        rank = 10;
        ui = labelled group.ssh "Machine SSH keys";
      };

      # Machine git identity (modules/core/git.nix): homeserver builds/fetches
      # with it (private plugins), Hermes commits and clones with it.
      # userName/userEmail = system gitconfig user.name/user.email.
      options.neo.core.git.userName = mkOption {
        type = types.str;
        default = "Neo";
        description = "Name shown on changes this server saves with git, such as your settings history and the assistant's work.";
        rank = 0;
        ui = summarized group.git;
      };

      options.neo.core.git.userEmail = lib.neo.mkInheritedOption {
        type = types.str;
        from = "core.hostname";
        default = "neo@${config.neo.core.hostname}";
        description = "Email address shown on changes this server saves with git.";
        rank = 10;
        ui = summarized group.git;
      };

      # Used for git over https (credential helper) and Nix github:/gitlab:
      # inputs (access-tokens). SSH URLs (git+ssh://git@host/owner/repo) use the
      # machine git SSH key instead.
      options.neo.core.git.tokens = lib.neo.mkSecretOption {
        type = types.attrsOf types.str;
        default = {};
        example = {"github.com" = "github_pat_…";};
        description = "Access tokens per git website (e.g. github.com and a read-only token) so the server can download your private plugins. GitLab tokens need the PAT: prefix.";
        rank = 20;
        ui = labelled group.git "Access tokens";
      };

      # Value = the host's ed25519 public key from `ssh-keyscan -t ed25519 <host>`
      # without the leading host name. Builtins: github.com, gitlab.com.
      options.neo.core.git.knownHosts = mkOption {
        type = types.attrsOf types.str;
        default = {};
        description = "Extra git servers the server may reach over SSH, each with its SSH host key so it can trust them. github.com and gitlab.com are already included. Use host:port for an unusual port (e.g. git.example.com:2222), and save a new server before adding a plugin stored on it.";
        rank = 30;
        ui = labelled group.advanced "Extra git servers";
      };

      # One identity for SSH, git and secret decryption (docs/design/secrets.md).
      # Off keeps the separate git key at /var/lib/neo/git/id_ed25519.
      options.neo.core.git.useHostKey = mkOption {
        type = types.bool;
        default = false;
        description = "Use the server's own SSH key for git instead of a separate git key. Git websites and servers then need the server's public key instead.";
        rank = 40;
        ui = labelled group.advanced "Use the server key for git";
      };

      options.neo.core.git.users = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Local users that use the machine git identity (SSH key and tokens). Modules append themselves (core: homeserver; Hermes: hermes).";
        internal = true;
      };

      # Plugin instances keyed by id: { url = "github:user/plugin"; prefix = ""; }.
      # The old list of URLs still evaluates (ids from lib.neo.plugin.normalize).
      # Each id is the flake input plugin-<id> (templates/homeserver/modules/inputs.nix).
      options.neo.core.plugins = mkOption {
        type = types.coercedTo (types.listOf types.str) lib.neo.plugin.normalize (types.attrsOf (types.submodule {
          options = {
            url = mkOption {
              type = types.str;
              description = "Where the plugin comes from, e.g. github:user/plugin or github:user/plugin/branch.";
            };
            prefix = mkOption {
              type = types.strMatching lib.neo.plugin.prefixPattern;
              default = "";
              description = "Put in front of the plugin's app names so it can be added twice, e.g. dev turns portrait into dev-portrait. Lowercase letters and digits, starting with a letter. Empty keeps the names.";
            };
          };
        }));
        default = {};
        description = "Add-ons that bring extra apps or features, each given by its address (e.g. github:user/plugin).";
        rank = 5;
        ui = lib.neo.ui.mkUi {
          widget = "pluginList";
          emptyHint = "No plugins yet. Add a flake URL (github:user/repo, git+file:/path, or path:/path).";
        };
      };

      options.neo.core.hostname = mkOption {
        type = types.str;
        default = "nixos";
        description = "Name of this server on your network, e.g. homeserver.";
        rank = 10;
        ui = ui.mkUi {label = "Server name";};
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
        description = "Your time zone, e.g. Europe/Zurich.";
        rank = 30;
      };

      # networking.timeServers, used by systemd-timesyncd. The Exact time
      # service (chrony, services/ntp) replaces timesyncd and has its own list.
      options.neo.core.timeServers = mkOption {
        type = types.listOf types.str;
        default = ["pool.ntp.org"];
        description = "Internet time servers this server sets its clock from. The default works worldwide; while Exact time is on, its own time servers are used instead.";
        rank = 31;
        ui = labelled group.advanced "Time servers";
      };

      # zramSwap: compressed swap in RAM.
      options.neo.core.zramSwap.enabled = mkEnableOption "compressed memory swap, which helps the server cope when memory runs low. Recommended with 8 GB of RAM or less" {
        rank = 35;
        ui = ui.mkUi {label = "Compressed memory swap";};
      };

      # Owner of the homeserver user and of all OCI appdata; admin gets uid + 1.
      options.neo.core.uid = mkOption {
        type = types.int;
        default = 1000;
        description = "User number that apps run as and that owns their files. Do not change after setup: existing files keep the old owner.";
        rank = 40;
        ui = labelled group.advanced "User ID";
      };

      options.neo.core.gid = mkOption {
        type = types.int;
        default = 1000;
        description = "Group number that apps run as and that owns their files. Do not change after setup: existing files keep the old group.";
        rank = 50;
        ui = labelled group.advanced "Group ID";
      };

      # Sets nix.settings.max-jobs (parallel Nix derivations) when non-null.
      options.neo.core.nix.maxJobs = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "How many builds may run at once when the server updates itself. Empty lets Nix decide; set 1 with 8 GB of RAM or less.";
        rank = 60;
        ui = ui.mkUi {
          group = group.advanced;
          label = "Parallel builds";
          emptyMeans = "Nix decides";
        };
      };

      # Sets nix.settings.cores (cores per Nix build job) when non-null.
      options.neo.core.nix.cores = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "How many processor cores each build may use. Empty lets Nix decide; set 1 with 8 GB of RAM or less.";
        rank = 70;
        ui = ui.mkUi {
          group = group.advanced;
          label = "Cores per build";
          emptyMeans = "Nix decides";
        };
      };

      # Remote Nix builders: heavy builds (including nixosConfigurations) are
      # delegated over SSH so a weak local machine only evaluates and fetches results.
      # Multiple machines can be configured; speedFactor sets priority weight.
      options.neo.core.nix.builders = mkOption {
        type = types.attrsOf (types.submodule {
          options =
            {
              enabled = mkOption {
                type = types.bool;
                default = true;
                description = "Use this computer for remote builds. Turn off to pause it without removing its settings.";
                rank = 0;
              };

              port = mkOption {
                type = types.nullOr types.port;
                default = null;
                description = "SSH port on the build computer. Leave empty for the standard port.";
                rank = 15;
                ui = ui.mkUi {
                  label = "SSH port";
                  emptyMeans = "22";
                };
              };

              sshOptions = mkOption {
                type = types.attrsOf types.str;
                default = {};
                example = {StrictHostKeyChecking = "accept-new";};
                description = "Extra SSH config options for this computer (option name and value, e.g. StrictHostKeyChecking = accept-new). Usually empty.";
                rank = 40;
                ui = ui.mkUi {label = "SSH options";};
              };

              maxJobs = mkOption {
                type = types.ints.positive;
                default = 8;
                description = "How many builds this computer runs at once, roughly its number of processor cores.";
                rank = 50;
                ui = ui.mkUi {label = "Parallel builds";};
              };

              # Nix buildMachines.speedFactor: higher weight is preferred when
              # several build computers are enabled.
              speedFactor = mkOption {
                type = types.ints.positive;
                default = 1;
                description = "Weight for prioritizing this computer over other build computers; higher numbers are preferred.";
                rank = 60;
                ui = ui.mkUi {label = "Weight";};
              };

              systems = mkOption {
                type = types.listOf types.str;
                default = ["x86_64-linux"];
                description = "Processor types this computer can build for, e.g. x86_64-linux or aarch64-linux.";
                rank = 70;
                ui = ui.mkUi {label = "Systems";};
              };

              supportedFeatures = mkOption {
                type = types.listOf types.str;
                default = ["nixos-test" "benchmark" "big-parallel" "kvm"];
                description = "Special build features this computer offers. Leave as is unless you know it differs.";
                rank = 80;
                ui = ui.mkUi {label = "Supported features";};
              };

              mandatoryFeatures = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Only send builds that require all of these features to this computer. Usually empty.";
                rank = 85;
                ui = ui.mkUi {label = "Mandatory features";};
              };

              # Base64 of the key type and key fields of an ssh-keyscan line.
              publicHostKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Optional SSH host key of this computer (base64), so the server can trust it without asking.";
                rank = 90;
                ui = ui.mkUi {
                  label = "SSH host key";
                  emptyMeans = "System known hosts";
                };
              };
            }
            // (let
              ssh = lib.neo.mkSshConnectionOptions {
                # After enabled (0): host/user/sshKey/extra = 10–40, then maxJobs…
                rankBase = 10;
                hostDescription = "Name or IP address of the build computer.";
                # Must be in nix.trusted-users there.
                userDescription = "User account on the build computer. It must be allowed to run builds there.";
                # The nix-daemon (root) uses this key.
                sshKeyDescription = "Key the server logs in with. Leave as is to use the server's own homeserver key.";
              };
            in {
              host = ssh.host // {default = "";};
              user = ssh.user // {default = "homeserver";};
              inherit (ssh) sshKey;
            });
        });
        default = {};
        example = literalExpression ''
          {
            workstation = {
              host = "192.168.1.10";
              user = "homeserver";
              maxJobs = 16;
              speedFactor = 4;
            };
            nas = {
              host = "192.168.1.20";
              user = "homeserver";
              maxJobs = 4;
              speedFactor = 1;
            };
          }
        '';
        description = "Other computers that do the heavy building for updates over SSH, so a small server only downloads the results. Add more than one and raise a computer's weight to prefer it.";
        rank = 81;
        ui = labelled group.advanced "Remote build computers";
      };

      # Changing a volume path only changes where services look; activation
      # creates the new directory but nothing is moved.
      options.neo.core.volumes.root = mkOption {
        type = types.str;
        default = "/var/neo";
        description = "Main folder for all Neo data; the other folders sit inside it by default. Changing it does not move existing files.";
        rank = 89;
        ui = ui.mkUi {
          group = group.volumes;
          label = "Main folder";
          summary = true;
        };
      };

      options.neo.core.volumes.appdata = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/AppData";
        description = "Folder where apps keep their settings and databases. Changing it does not move existing files.";
        rank = 90;
        ui = labelled group.volumes "App data folder";
      };

      options.neo.core.volumes.data = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA";
        description = "Folder for your own files; by default Media, Documents and app data sit inside it. Changing it does not move existing files.";
        rank = 100;
        ui = labelled group.volumes "Data folder";
      };

      options.neo.core.volumes.media = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Media";
        description = "Folder for photos, music, films and other media that apps share. Changing it does not move existing files.";
        rank = 110;
        ui = labelled group.volumes "Media folder";
      };

      options.neo.core.volumes.documents = mkOption {
        type = types.str;
        default = "${config.neo.core.volumes.root}/DATA/Documents";
        description = "Folder for documents that apps share. Changing it does not move existing files.";
        rank = 120;
        ui = labelled group.volumes "Documents folder";
      };

      options.neo.migrations.applied = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Applied settings.toml migrations (managed by neo migrate command).";
        internal = true;
      };
    };
}
