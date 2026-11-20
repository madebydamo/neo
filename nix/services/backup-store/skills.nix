# Hermes skill for backup-store.
{...}: {
  flake.modules.nixos.backup-store-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.backup-store.skill.conf = lib.neo.mkServiceSkill {
      service = "backup-store";
      inherit config;
      description = "Stores other Neo servers' backups, reachable only over HTTPS: plain files, encrypted (restic) or ZFS snapshots";
      tags = ["neo" "backup"];
      body = ''
        ## When to Use
        Adding a server that backs up here, store storage or encryption, failed backups that target this store.

        ## Architecture notes
        - Chain: https://<subdomain>.<domain> → SWAG → backup-store-tunnel (websocat) → backup-store-sshd on 127.0.0.1 → neo-store-gate
        - The same address is the rescue portal for browsers: SWAG sends WebSocket requests to the tunnel (no login) and everything else to the Neo web UI (login), which serves only the portal there
        - backup-store-report (timer, 15 min) writes /var/lib/neo-backup-store/report.json: last backup, versions, size, last connection per client
        - Each client is one account backup-<client>, home <path>/<client> (0700), one format; the gate refuses every other command
        - Storage is outside the Neo data volume, so this server's own backup never includes it
        - Dataset storage: <dataset>/<client>; ZFS backups go to <dataset>/<client>/<name>
        - Client side: a destination in services.backup.destinations with target = "store", storeUrl, storeClient

        ## Credentials
        - Settings: `services.backup-store.clients.<client>.publicKeys`: the client's host key (System → SSH there). Encrypted clients need nothing else: the keys in System → SSH on the client get read-only access here after its next backup run (it uploads `neo-backup.recipients`). Other formats: add an admin key to restore with
        - Settings: `services.backup-store.encryptionKey` when the dataset is encrypted

        ## Procedures
        1. Add a client: clients.<client> with format and publicKeys, apply; on the client add a destination with target = "store", storeUrl = https://<subdomain>.<domain>
        2. Run a backup on the client: `systemctl start backup`, read `journalctl -u backup`
        3. Here: `journalctl -u backup-store-sshd` for logins and refused commands, `journalctl -u backup-store-setup` for storage
        4. Restore a lost client: send the user to the portal https://<subdomain>.<domain>, page of that client. It lists what they need and the command. "From this server" needs their admin private key, which they paste there themselves: never ask for it or handle it

        ## Pitfalls
        - A dataset on the main pool is erased by neo restore of this server
        - Files clients: owners are kept in extended attributes (rsync --fake-super); the folder shows the account as owner, a restore through rsync brings the real owners back
        - Losing the encryption key loses the stored backups of an encrypted dataset; a dataset cannot be encrypted in place
        - Removing a client keeps its data at <path>/<client>

        ## Verification
        - `systemctl is-active backup-store-sshd backup-store-tunnel`; `curl -sk -o /dev/null -w '%{http_code}' https://<subdomain>.<domain>/` answers 302 (login) for a browser; a WebSocket request reaches the tunnel
      '';
    };
  };
}
