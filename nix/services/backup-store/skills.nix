# Hermes skill for backup-store.
{...}: {
  flake.modules.nixos.backup-store-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.backup-store;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.backup-store.skill.conf = lib.neo.mkServiceSkill {
      service = "backup-store";
      inherit cfg domain;
      description = "Stores other Neo servers' backups, reachable only over HTTPS: plain files, encrypted (restic) or ZFS snapshots";
      tags = ["neo" "backup"];
      body = ''
        ## When to Use
        Adding a server that backs up here, store storage or encryption, failed backups that target this store.

        ## Architecture notes
        - Chain: https://<subdomain>.<domain> → SWAG → backup-store-tunnel (websocat) → backup-store-sshd on 127.0.0.1 → neo-store-gate
        - Each client is one account backup-<client>, home <path>/<client> (0700), one format; the gate refuses every other command
        - Storage is outside the Neo data volume, so this server's own backup never includes it
        - Dataset storage: <dataset>/<client>; ZFS backups go to <dataset>/<client>/<name>
        - Client side: services.backup target = "store", storeUrl, storeClient

        ## Credentials
        - Settings: `services.backup-store.clients.<client>.publicKeys`: the client's host key (System → SSH there), plus an admin key to restore with
        - Settings: `services.backup-store.encryptionKey` when the dataset is encrypted

        ## Procedures
        1. Add a client: clients.<client> with format and publicKeys, apply; on the client set target = "store", storeUrl = https://<subdomain>.<domain>
        2. Run a backup on the client: `systemctl start backup`, read `journalctl -u backup`
        3. Here: `journalctl -u backup-store-sshd` for logins and refused commands, `journalctl -u backup-store-setup` for storage

        ## Pitfalls
        - A dataset on the main pool is erased by neo restore of this server
        - Files clients: owners are kept in extended attributes (rsync --fake-super); the folder shows the account as owner, a restore through rsync brings the real owners back
        - Losing the encryption key loses the stored backups of an encrypted dataset; a dataset cannot be encrypted in place
        - Removing a client keeps its data at <path>/<client>

        ## Verification
        - `systemctl is-active backup-store-sshd backup-store-tunnel`; `curl -sk -o /dev/null -w '%{http_code}' https://<subdomain>.<domain>/` answers from websocat, not tinyauth
      '';
    };
  };
}
