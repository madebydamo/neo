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

      # Enabled destinations by (priority, name), the order neo restore tries
      # them in (restore/backup.rs); the backup runs in it too.
      destinations = sortOn (d: d.priority) (
        mapAttrsToList (name: d: d // {inherit name;}) (filterAttrs (_: d: d.enabled) cfg.destinations)
      );
      formats = unique (map (d: d.format) destinations);

      # https://host[:port]: SSH inside a WebSocket (services.backup-store).
      storeAuthority = d: head (splitString "/" (removePrefix "https://" d.storeUrl));
      storeHost = d: head (splitString ":" (storeAuthority d));
      storeOptions = d: [
        "-o"
        "ProxyCommand=${pkgs.writeShellScript "neo-backup-store-tunnel" ''
          exec ${getExe pkgs.websocat} --binary -E - ${escapeShellArg "wss://${storeAuthority d}/"}
        ''}"
        "-o"
        "ServerAliveInterval=30"
        # The store is authenticated by its TLS certificate on first contact.
        "-o"
        "StrictHostKeyChecking=accept-new"
        "-o"
        "UserKnownHostsFile=/var/lib/neo-backup/known_hosts"
      ];

      # ssh takes the first value of an option, so extraOptions win.
      sshCommand = d:
        concatStringsSep " " (
          ["${pkgs.openssh}/bin/ssh" "-i" (escapeShellArg d.sshKey) "-o" "BatchMode=yes"]
          ++ d.extraOptions
          ++ optionals (d.target == "store") (storeOptions d)
        );

      # One file of shell variables per destination, sourced by backup.sh.
      destinationVars = d:
        pkgs.writeText "neo-backup-${d.name}.sh" (toShellVars {
          NEO_NAME = d.name;
          # A store is SSH with another way in.
          NEO_TARGET =
            if d.target == "store"
            then "ssh"
            else d.target;
          NEO_FORMAT = d.format;
          NEO_DEST = d.remotePath;
          NEO_DEST_DATASET = d.dataset;
          NEO_KEEP = toString d.keep;
          NEO_SSH_DEST =
            if d.target == "store"
            then "backup-${d.storeClient}@${storeHost d}"
            else "${d.user}@${d.host}";
          NEO_SSH = sshCommand d;
          NEO_LOG = "${removeSuffix ".log" (toString cfg.logFile)}-${d.name}.log";
        });

      # Where a destination writes, for the duplicate check.
      place = d:
        concatStringsSep ":" [
          (
            if d.target == "store"
            then "store/${d.storeClient}@${storeHost d}"
            else if d.target == "ssh"
            then "${d.user}@${d.host}"
            else "disk"
          )
          (
            if d.format == "zfs"
            then "zfs/${d.dataset}"
            else d.remotePath
          )
        ];
      duplicates = let
        places = map place destinations;
      in
        filter (d: count (p: p == place d) places > 1) destinations;

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
          then "Neo backup to ${concatMapStringsSep ", " (d: d.name) destinations}"
          else "Neo backup: new encryption key and a new full backup";
        after = ["network-online.target"];
        wants = ["network-online.target"];
        environment = {
          # name=file, in run order.
          NEO_DESTINATIONS = concatMapStringsSep "\n" (d: "${d.name}=${destinationVars d}") destinations;
          # The ZFS destinations whose base snapshot stays here.
          NEO_ZFS_ACTIVE = concatMapStringsSep "\n" (d: d.name) (filter (d: d.format == "zfs") destinations);
          NEO_ZFS_ALL = concatStringsSep "\n" (attrNames (filterAttrs (_: d: d.format == "zfs") cfg.destinations));
          NEO_SOURCE_DIR = toString cfg.sourceDir;
          NEO_ROOT = root;
          # The Neo data dataset holds volumes.root unless an additional disk
          # replaces it (then the files format copies the live folder).
          NEO_DATASET =
            if disko.enabled && !(elem root (attrValues disko.additionalDisks)) && hasPrefix root (toString cfg.sourceDir)
            then "${disko.poolName}/neo"
            else "";
          NEO_EXCLUDES = concatStringsSep "\n" cfg.excludedDirs;
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
          # The store's SSH host key, once seen; the ZFS base per destination.
          StateDirectory = "neo-backup";
          StateDirectoryMode = "0700";
        };
      };
    in {
      config = mkIf cfg.enabled {
        assertions =
          [
            {
              assertion = destinations != [];
              message = "neo.services.backup: add a destination (services.backup.destinations.<name>).";
            }
            {
              assertion = duplicates == [];
              message = "neo.services.backup: destinations ${concatMapStringsSep ", " (d: d.name) duplicates} write to the same place; give each its own folder or dataset.";
            }
          ]
          ++ concatMap (d: let
            at = "neo.services.backup.destinations.${d.name}";
          in [
            {
              assertion = d.format != "zfs" || disko.enabled;
              message = "${at}: the ZFS format needs the ZFS disk layout (neo.disko.enabled).";
            }
            {
              assertion = d.format != "zfs" || d.dataset != "";
              message = "${at}: set dataset (e.g. tank/backup/${config.neo.core.hostname}) for the ZFS format.";
            }
            {
              assertion = d.target != "ssh" || d.host != "";
              message = "${at}: set the backup server (host).";
            }
            {
              assertion = d.target != "store" || hasPrefix "https://" d.storeUrl;
              message = "${at}: storeUrl must start with https:// (the store is only reachable over HTTPS).";
            }
            {
              assertion = d.target != "disk" || d.format == "zfs" || hasPrefix "/" d.remotePath;
              message = "${at}: on a disk, the backup folder must be an absolute path (where the disk is mounted).";
            }
          ])
          destinations;
        warnings =
          optional (elem "encrypted" formats && adminKeys == []) ''
            neo.services.backup: the encrypted backup can only be opened with this server's own key, because System → SSH has no ssh-ed25519 key. Add yours, or a lost server means a lost backup.''
          ++ concatMap (d:
            optional (let
              store = config.neo.services.backup-store;
            in
              d.target == "store" && store.enabled && storeHost d == "${toString store.subdomain}.${toString config.neo.services.swag.domain}") ''
              neo.services.backup.destinations.${d.name}: the backup goes to this server's own backup store: if this server is lost, so is that copy.'')
          destinations;

        systemd.services =
          {backup = unit "backup";}
          # `neo backup rotate-key`: new data key, new repository (full backup),
          # then the old repository is removed. Not in systemdUnits: the
          # service target must never start it.
          // optionalAttrs (elem "encrypted" formats) {
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
