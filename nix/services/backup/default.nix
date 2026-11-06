# Backup service implementation (docs/design/backup-restore.md). The script is
# backup.sh; `neo restore` reads the same settings to bring the data back.
{...}: {
  flake.modules.nixos.backup = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.backup;
      disko = config.neo.disko;
      root = config.neo.core.volumes.root;

      isStore = cfg.target == "store";
      # https://host[:port]: SSH inside a WebSocket (services.backup-store).
      storeAuthority = head (splitString "/" (removePrefix "https://" cfg.storeUrl));
      storeHost = head (splitString ":" storeAuthority);
      storeTunnel = pkgs.writeShellScript "neo-backup-store-tunnel" ''
        exec ${getExe pkgs.websocat} --binary -E - ${escapeShellArg "wss://${storeAuthority}/"}
      '';
      storeOptions = [
        "-o"
        "ProxyCommand=${storeTunnel}"
        "-o"
        "ServerAliveInterval=30"
        # The store is authenticated by its TLS certificate on first contact.
        "-o"
        "StrictHostKeyChecking=accept-new"
        "-o"
        "UserKnownHostsFile=/var/lib/neo-backup/known_hosts"
      ];

      # ssh takes the first value of an option, so extraOptions win.
      sshCommand = concatStringsSep " " (
        ["${pkgs.openssh}/bin/ssh" "-i" (escapeShellArg cfg.sshKey) "-o" "BatchMode=yes"]
        ++ cfg.extraOptions
        ++ optionals isStore storeOptions
      );

      # Admin keys that open neo-backup.age (encrypted format), with the host
      # key: the same recipients as secrets.sops.yaml.
      adminKeys = concatMap (k: let
        p = filter (x: x != "") (splitString " " k);
      in
        optional (length p >= 2 && head p == "ssh-ed25519") "ssh-ed25519 ${elemAt p 1}")
      (unique config.neo.core.ssh.authorizedKeys);

      backup = pkgs.writeShellApplication {
        name = "neo-backup";
        runtimeInputs = with pkgs; [coreutils findutils gnugrep gnused rsync openssh util-linux restic age config.boot.zfs.package];
        bashOptions = ["errexit" "nounset" "pipefail"];
        text = builtins.readFile ./backup.sh;
      };

      unit = name: {
        description =
          if name == "backup"
          then "Neo backup (${cfg.format} to ${cfg.target})"
          else "Neo backup: new encryption key and a new full backup";
        after = ["network-online.target"];
        wants = ["network-online.target"];
        environment = {
          # A store is SSH with another way in.
          NEO_TARGET =
            if isStore
            then "ssh"
            else cfg.target;
          NEO_FORMAT = cfg.format;
          NEO_SOURCE_DIR = toString cfg.sourceDir;
          NEO_ROOT = root;
          # The Neo data dataset holds volumes.root unless an additional disk
          # replaces it (then the files format copies the live folder).
          NEO_DATASET =
            if disko.enabled && !(elem root (attrValues disko.additionalDisks)) && hasPrefix root (toString cfg.sourceDir)
            then "${disko.poolName}/neo"
            else "";
          NEO_DEST = cfg.remotePath;
          NEO_DEST_DATASET = cfg.dataset;
          NEO_KEEP = toString cfg.keep;
          NEO_SSH_DEST =
            if isStore
            then "backup-${cfg.storeClient}@${storeHost}"
            else "${cfg.user}@${cfg.host}";
          NEO_SSH = sshCommand;
          NEO_EXCLUDES = concatStringsSep "\n" cfg.excludedDirs;
          NEO_LOG = cfg.logFile;
          NEO_HOSTNAME = config.neo.core.hostname;
          NEO_HOST_KEY = config.neo.core.secrets.identityFile;
          NEO_RECIPIENTS = concatStringsSep "\n" adminKeys;
          NEO_ROTATE = boolToString (name == "backup-rotate-key");
        };
        serviceConfig = {
          Type = "oneshot";
          ExecStart = getExe backup;
          # Decrypted data key and the ssh wrapper (encrypted format).
          RuntimeDirectory = name;
          RuntimeDirectoryMode = "0700";
          # restic's metadata cache (encrypted like the repository).
          CacheDirectory = "neo-backup";
          CacheDirectoryMode = "0700";
          # The store's SSH host key, once seen.
          StateDirectory = "neo-backup";
          StateDirectoryMode = "0700";
        };
      };
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfg.format != "zfs" || disko.enabled;
            message = "neo.services.backup: the ZFS format needs the ZFS disk layout (neo.disko.enabled).";
          }
          {
            assertion = cfg.format != "zfs" || cfg.dataset != "";
            message = "neo.services.backup: set dataset (e.g. tank/backup/${config.neo.core.hostname}) for the ZFS format.";
          }
          {
            assertion = cfg.target != "ssh" || cfg.host != "";
            message = "neo.services.backup: set the backup server (host).";
          }
          {
            assertion = !isStore || hasPrefix "https://" cfg.storeUrl;
            message = "neo.services.backup: storeUrl must start with https:// (the store is only reachable over HTTPS).";
          }
          {
            assertion = cfg.target != "disk" || cfg.format == "zfs" || hasPrefix "/" cfg.remotePath;
            message = "neo.services.backup: on a disk, the backup folder must be an absolute path (where the disk is mounted).";
          }
        ];
        warnings =
          optional (cfg.format == "encrypted" && adminKeys == []) ''
            neo.services.backup: the encrypted backup can only be opened with this server's own key, because System → SSH has no ssh-ed25519 key. Add yours, or a lost server means a lost backup.''
          ++ optional (let
            store = config.neo.services.backup-store;
          in
            isStore && store.enabled && storeHost == "${toString store.subdomain}.${toString config.neo.services.swag.domain}") ''
            neo.services.backup: the backup goes to this server's own backup store: if this server is lost, so is the backup.'';

        systemd.services =
          {backup = unit "backup";}
          # `neo backup rotate-key`: new data key, new repository (full backup),
          # then the old repository is removed. Not in systemdUnits: the
          # service target must never start it.
          // optionalAttrs (cfg.format == "encrypted") {
            backup-rotate-key = unit "backup-rotate-key";
          };

        systemd.timers.backup = {
          wantedBy = ["timers.target"];
          partOf = ["backup.service"];
          timerConfig = {
            OnCalendar = cfg.schedule;
            Persistent = true;
            Unit = "backup.service";
          };
        };
      };
    };
}
