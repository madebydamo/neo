# Backup store options (docs/design/backup-restore.md §6).
{...}: {
  flake.modules.nixos.backup-store-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.backup-store = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Backup store: other servers keep their backups here, reachable only over HTTPS" {rank = 0;};

              storage = mkOption {
                type = types.enum ["dataset" "folder"];
                default = "dataset";
                rank = 10;
                description = "Where the backups are kept. A ZFS dataset takes every backup format and can be encrypted; a folder works on any disk, but not for ZFS snapshot backups.";
                ui = lib.neo.ui.mkUi {
                  label = "Keep backups in";
                  choiceLabels = {
                    dataset = "A ZFS dataset";
                    folder = "A folder";
                  };
                };
              };

              # Created by backup-store-setup; a pool other than the main one
              # is imported at boot.
              dataset = mkOption {
                type = types.str;
                default = "${config.neo.disko.poolName}/backup-store";
                example = "tank/backup-store";
                rank = 11;
                description = "ZFS dataset for the backups, starting with the pool name. It is created when missing. On this server's main disk, a restore of this server erases it, so prefer a pool on another disk.";
                ui = lib.neo.ui.mkUi {
                  label = "Dataset";
                  visibleWhen = "storage=dataset";
                };
              };

              # Mountpoint of the dataset, or the folder itself. Never inside
              # volumes.root: this server's own backup would copy the backups.
              path = mkOption {
                type = types.str;
                default = "/var/backup-store";
                example = "/mnt/backups";
                rank = 12;
                description = "Folder that holds the backups, usually on its own disk. It must be outside the data volume, so this server's own backup doesn't copy them.";
                ui = lib.neo.ui.mkUi {label = "Folder";};
              };

              encrypted = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = "Encrypt the dataset, so a stolen disk reveals nothing. This server can still read backups that were not already encrypted by the client. Only for a new dataset: ZFS cannot encrypt an existing one.";
                ui = lib.neo.ui.mkUi {
                  label = "Encrypt the stored backups";
                  visibleWhen = "storage=dataset";
                };
              };

              # zfs keyformat=passphrase; backup-store-setup loads it at boot and
              # re-wraps the dataset key when it changes.
              encryptionKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 21;
                helper = lib.neo.helpers.randomToken;
                description = "Key that opens the encrypted dataset, at least 8 characters. Keep a copy elsewhere: without it, the stored backups are lost. Changing it later is safe while the dataset is open.";
                ui = lib.neo.ui.mkUi {
                  label = "Encryption key";
                  visibleWhen = "encrypted";
                };
              };

              clients = mkOption {
                type = types.attrsOf (types.submodule {
                  options = {
                    format = mkOption {
                      type = types.enum ["files" "encrypted" "zfs"];
                      default = "encrypted";
                      rank = 0;
                      description = "Backup format this server uses; it must match the setting on that server. ZFS snapshots need a dataset here.";
                      ui = lib.neo.ui.mkUi {
                        label = "Backup format";
                        choiceLabels = {
                          files = "Plain files";
                          encrypted = "Encrypted";
                          zfs = "ZFS snapshots";
                        };
                      };
                    };
                    publicKeys = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      rank = 10;
                      description = "SSH public keys allowed in: that server's own key (System → SSH there), and your own if you want to restore with it.";
                      ui = lib.neo.ui.mkUi {label = "SSH public keys";};
                    };
                  };
                });
                default = {};
                rank = 30;
                example = literalExpression ''{ myhome = { format = "encrypted"; publicKeys = [ "ssh-ed25519 AAAA…" ]; }; }'';
                description = "Servers that may store their backups here, by name. Each gets its own private space and can only run backup commands in it.";
                ui = lib.neo.ui.mkUi {label = "Servers";};
              };

              # backup-store-tunnel: 127.0.0.1, reached by SWAG only.
              tunnelPort = mkOption {
                type = types.port;
                internal = true;
                default = 18022;
                description = "Internal port of the WebSocket tunnel";
              };
              # backup-store-sshd: 127.0.0.1, reached by the tunnel only.
              sshPort = mkOption {
                type = types.port;
                internal = true;
                default = 18023;
                description = "Internal port of the store's SSH server";
              };
            }
            # No tinyauth: SSH clients authenticate with their key inside the tunnel.
            // lib.neo.mkReverseProxyOptions {
              subdomain = "backup";
              auth = {
                available = false;
                enabled = false;
              };
            }
            // lib.neo.mkSystemdUnits ["backup-store-setup" "backup-store-sshd" "backup-store-tunnel"]
            // lib.neo.mkServiceMeta {
              category = "Files";
              license = {
                spdx = "MIT";
                url = "https://github.com/madebydamo/neo/blob/master/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Keep other servers' backups here, reachable only over HTTPS";
              description = ''
                Lets your other servers, or a friend's, keep their nightly backup on this one. It works even where only web traffic gets through.
                Each server gets its own private space and can do nothing else here.
              '';
              notes = ''
                On the other server: Backup → Back up to "A Neo backup store", the address https://<subdomain>.<domain> and the server name you used here. Its own key is under System → SSH there.
                ZFS snapshot backups: set that server's Backup dataset to <dataset>/<server name>/neo.
                Backups live outside the data volume, so this server's own backup does not copy them. A dataset on this server's main disk is erased when this server is restored.
                Removing a server stops its access but keeps its backups in <folder>/<server name>.
              '';
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Backup store for other servers.";
      };
    };
}
