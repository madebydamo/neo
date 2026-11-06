# Backup store (docs/design/backup-restore.md §6): HTTPS → SWAG → websocat →
# its own sshd on loopback → neo-store-gate, one account per client. The backup
# service and `neo restore` reach it with a websocat ProxyCommand and otherwise
# use their normal SSH paths.
{self, ...}: {
  flake.modules.nixos.backup-store = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.backup-store;
      root = toString config.neo.core.volumes.root;
      backupCfg = config.neo.services.backup;
      isDataset = cfg.storage == "dataset";
      pool = head (splitString "/" cfg.dataset);
      mainPool = config.neo.disko.poolName;
      neoDataset = "${mainPool}/neo";
      sftpServer = "${pkgs.openssh}/libexec/sftp-server";
      unwrapped = self.packages.${pkgs.stdenv.hostPlatform.system}.neo.unwrapped;
      keyFile = config.neo.core.secrets.templates."backup-store-key".path;

      inOrBelow = dir: p: p == dir || hasPrefix "${removeSuffix "/" dir}/" p;
      # `path` on its own mount (a disk from additionalDisks or fileSystems)?
      ownMount =
        any (m: m != "/" && m != "/boot" && m != "/nix" && inOrBelow m cfg.path)
        (attrNames config.fileSystems);

      gate = pkgs.writeShellScript "backup-store-gate" ''
        export PATH=${makeBinPath ([pkgs.rsync pkgs.coreutils] ++ optional isDataset config.boot.zfs.package)}
        export NEO_SFTP_SERVER=${sftpServer}
        exec ${unwrapped}/bin/neo-store-gate "$@"
      '';

      userOf = client: "backup-${client}";
      keysFile = client: c:
        concatMapStrings (key: ''
          restrict,command="${gate} ${c.format} ${
            if isDataset
            then "${cfg.dataset}/${client}"
            else "-"
          }" ${key}
        '')
        c.publicKeys;

      sshdConfig = pkgs.writeText "backup-store-sshd_config" ''
        Port ${toString cfg.sshPort}
        ListenAddress 127.0.0.1
        HostKey ${config.neo.core.secrets.identityFile}
        PidFile none
        AllowUsers ${concatMapStringsSep " " userOf (attrNames cfg.clients)}
        AuthorizedKeysFile /etc/neo/backup-store/authorized_keys/%u
        AuthenticationMethods publickey
        PasswordAuthentication no
        KbdInteractiveAuthentication no
        PermitRootLogin no
        UsePAM no
        AllowTcpForwarding no
        AllowAgentForwarding no
        AllowStreamLocalForwarding no
        X11Forwarding no
        PermitTunnel no
        ClientAliveInterval 30
        # Every connection comes from the tunnel, so a per-source penalty would
        # lock out all clients.
        PerSourcePenalties no
        # The authorized key's command replaces this; it marks the request for the gate.
        Subsystem sftp ${sftpServer}
      '';
    in {
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkDockerToLocalhostForward cfg.tunnelPort)
        {
          assertions =
            [
              {
                assertion = hasPrefix "/" cfg.path && cfg.path != "/";
                message = "neo.services.backup-store: path must be an absolute folder, not /.";
              }
              {
                assertion = !(inOrBelow root cfg.path) && !(inOrBelow cfg.path root);
                message = "neo.services.backup-store: path (${cfg.path}) must be outside the Neo data volume (${root}), or this server's backup would copy the stored backups.";
              }
              {
                assertion = !backupCfg.enabled || !(inOrBelow (toString backupCfg.sourceDir) cfg.path);
                message = "neo.services.backup-store: path (${cfg.path}) is inside the backup's Folder to back up (${toString backupCfg.sourceDir}).";
              }
              {
                assertion = !isDataset || (cfg.dataset != pool && !(inOrBelow neoDataset cfg.dataset));
                message = "neo.services.backup-store: dataset (${cfg.dataset}) must be a dataset of its own, not a pool or part of ${neoDataset}.";
              }
              {
                assertion = isDataset || !cfg.encrypted;
                message = "neo.services.backup-store: encryption needs a ZFS dataset (storage = \"dataset\").";
              }
              {
                # Its key is not there at pool import; see nix/modules/disko.
                assertion = !isDataset || !cfg.encrypted || config.boot.zfs.requestEncryptionCredentials != true;
                message = "neo.services.backup-store: an encrypted dataset stops this server from booting while boot.zfs.requestEncryptionCredentials is true (the Disko layout sets it to false). Set it to false or to the list of datasets to unlock at boot.";
              }
              {
                assertion = !isDataset || !cfg.encrypted || cfg.encryptionKey != null;
                message = "neo.services.backup-store: set encryptionKey for the encrypted dataset.";
              }
            ]
            ++ mapAttrsToList (client: c: {
              assertion = isDataset || c.format != "zfs";
              message = "neo.services.backup-store: client ${client} sends ZFS snapshots, which need a ZFS dataset (storage = \"dataset\").";
            })
            cfg.clients
            ++ mapAttrsToList (client: _: {
              assertion = match "[a-z0-9][a-z0-9-]{0,23}" client != null;
              message = "neo.services.backup-store: client name ${client} may only hold lowercase letters, digits and dashes (up to 24).";
            })
            cfg.clients;

          warnings =
            optional (isDataset && pool == mainPool && config.neo.disko.enabled) ''
              neo.services.backup-store: the stored backups are on this server's main disk (${cfg.dataset}): one disk failure or a neo restore of this server loses them.''
            ++ optional (!isDataset && !ownMount) ''
              neo.services.backup-store: ${cfg.path} is on the system disk: one disk failure or a reinstall of this server loses the stored backups.'';

          boot.supportedFilesystems = mkIf isDataset ["zfs"];
          # Pools of the Disko layout are imported already.
          boot.zfs.extraPools = mkIf (isDataset && !(elem pool (attrNames config.disko.devices.zpool))) [pool];

          users.groups.backup-store = {};
          users.users = mapAttrs' (client: _:
            nameValuePair (userOf client) {
              isSystemUser = true;
              group = "backup-store";
              home = "${cfg.path}/${client}";
              createHome = false;
              # Not "!": without PAM, sshd refuses a locked account outright.
              hashedPassword = "*";
              # sshd starts the authorized key's command through the login shell.
              shell = pkgs.bash;
            })
          cfg.clients;

          environment.etc = mapAttrs' (client: c:
            nameValuePair "neo/backup-store/authorized_keys/${userOf client}" {
              text = keysFile client c;
              # A real file: StrictModes refuses a symlink into /nix/store.
              mode = "0444";
            })
          cfg.clients;

          neo.core.secrets.templates."backup-store-key" = mkIf (isDataset && cfg.encrypted) {
            content = cfg.encryptionKey;
            owner = "root";
            mode = "0400";
            restartUnits = ["backup-store-setup.service"];
          };

          systemd.services.backup-store-setup = lib.neo.mkSetupService {
            inherit pkgs;
            name = "backup-store-setup";
            description = "Backup store: storage and client folders";
            blocking = true;
            retryInterval = 10;
            maxAttempts = 3;
            after = optionals isDataset ["zfs-import.target" "zfs-mount.service"];
            path = [pkgs.coreutils] ++ optional isDataset config.boot.zfs.package;
            environment = {
              NEO_STORAGE = cfg.storage;
              NEO_PATH = cfg.path;
              NEO_DATASET = cfg.dataset;
              NEO_ENCRYPTED = boolToString (isDataset && cfg.encrypted);
              NEO_KEY_FILE = optionalString (isDataset && cfg.encrypted) keyFile;
              NEO_CLIENTS = concatStringsSep "\n" (mapAttrsToList (client: c: "${client} ${c.format}") cfg.clients);
            };
            # A declared mount for the folder must be there, or backups would
            # land on the system disk below an empty mountpoint.
            unitConfig.RequiresMountsFor = mkIf (!isDataset) cfg.path;
            serviceConfig.TimeoutStartSec = 120;
            script = builtins.readFile ./setup.sh;
          };

          systemd.services.backup-store-sshd = {
            description = "Backup store: SSH server behind the HTTPS tunnel";
            wantedBy = ["multi-user.target"];
            after = ["backup-store-setup.service"];
            requires = ["backup-store-setup.service"];
            serviceConfig = {
              ExecStart = "${pkgs.openssh}/bin/sshd -D -e -f ${sshdConfig}";
              Restart = "on-failure";
              # A settings change must not cut a running backup.
              KillMode = "process";
              # Sessions inherit this: only the store is writable, and with the
              # accounts' 0700 homes only their own part of it.
              ProtectSystem = "strict";
              ProtectHome = true;
              PrivateTmp = true;
              ReadWritePaths = [cfg.path];
              InaccessiblePaths = ["-${root}" "-/var/lib"];
            };
          };

          systemd.services.backup-store-tunnel = {
            description = "Backup store: WebSocket tunnel to its SSH server";
            wantedBy = ["multi-user.target"];
            after = ["network.target"];
            serviceConfig = {
              ExecStart = "${getExe pkgs.websocat} --binary -E ws-l:127.0.0.1:${toString cfg.tunnelPort} tcp:127.0.0.1:${toString cfg.sshPort}";
              Restart = "always";
              DynamicUser = true;
              ProtectSystem = "strict";
              ProtectHome = true;
              PrivateTmp = true;
              NoNewPrivileges = true;
              RestrictAddressFamilies = ["AF_INET" "AF_INET6"];
            };
          };
        }
      ]);
    };
}
