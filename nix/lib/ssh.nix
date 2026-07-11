# Shared SSH connection options (host, user, key, extra flags) for backup, remote builds, etc.
{lib, ...}: {
  libExtensions.ssh = {
    neo = {
      # Default path of the auto-generated homeserver ed25519 key (see modules/core/base.nix).
      defaultHomeserverSshKey = "/home/homeserver/.ssh/id_ed25519";

      # Merge into a submodule `options = { ... } // lib.neo.mkSshConnectionOptions { ... };`.
      # `sshKey` defaults to the auto-generated homeserver key and may be overridden per use site.
      mkSshConnectionOptions = {
        hostRank ? 20,
        userRank ? 30,
        sshKeyRank ? 10,
        extraOptionsRank ? 92,
        defaultSshKey ? "/home/homeserver/.ssh/id_ed25519",
        hostDescription ? "Remote hostname or IP",
        userDescription ? "Username for the SSH connection",
        sshKeyDescription ? "Path to the SSH private key. Defaults to the auto-generated homeserver key (created at activation if missing).",
        extraOptionsDescription ? "Additional SSH options (e.g. -o StrictHostKeyChecking=accept-new)",
      }:
        with lib; {
          host =
            mkOption {
              type = types.str;
              description = hostDescription;
            }
            // {rank = hostRank;};

          user =
            mkOption {
              type = types.str;
              description = userDescription;
            }
            // {rank = userRank;};

          sshKey =
            mkOption {
              type = types.str;
              default = defaultSshKey;
              description = sshKeyDescription;
            }
            // {rank = sshKeyRank;};

          extraOptions =
            mkOption {
              type = types.listOf types.str;
              default = [];
              description = extraOptionsDescription;
            }
            // {rank = extraOptionsRank;};
        };
    };
  };
}
