# Shared SSH connection options (host, user, key, extra flags) for backup, remote builds, etc.
#
# Ranks are level-dependent (siblings only). Use `rankBase` so the four fields sit
# as a contiguous local band without inventing global numbers per call site:
#   host = rankBase, user = +10, sshKey = +20, extraOptions = +30
#
# Merge into a submodule:
#   options = { ... } // lib.neo.mkSshConnectionOptions {
#     rankBase = 10;  # after enabled = 0
#     hostDescription = "...";
#   };
{lib, ...}: {
  libExtensions.ssh = {
    neo = {
      # Default path of the auto-generated homeserver ed25519 key (see modules/core/base.nix).
      defaultHomeserverSshKey = "/home/homeserver/.ssh/id_ed25519";

      mkSshConnectionOptions = {
        # Local sibling band start (host). Other SSH fields step by 10.
        rankBase ? 10,
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
            // {rank = rankBase;};

          user =
            mkOption {
              type = types.str;
              description = userDescription;
            }
            // {rank = rankBase + 10;};

          sshKey =
            mkOption {
              type = types.str;
              default = defaultSshKey;
              description = sshKeyDescription;
            }
            // {rank = rankBase + 20;};

          extraOptions =
            mkOption {
              type = types.listOf types.str;
              default = [];
              description = extraOptionsDescription;
            }
            // {rank = rankBase + 30;};
        };
    };
  };
}
