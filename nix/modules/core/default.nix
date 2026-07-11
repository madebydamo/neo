# Core NixOS configuration: openssh, users, activation scripts, timezone, stateVersion.
{...}: {
  flake.modules.nixos.core = {
    config,
    lib,
    pkgs,
    ...
  }: {
    services.openssh = {
      enable = true;
      settings.PermitRootLogin = "yes";
    };

    virtualisation.docker.enable = true;
    virtualisation.oci-containers.backend = "docker";

    users.allowNoPasswordLogin = true;
    users.mutableUsers = false;

    boot.kernel.sysctl = {
      "net.ipv4.ip_forward" = 1;
      "net.ipv6.conf.all.forwarding" = 1;
    };

    systemd.services.NetworkManager-wait-online.enable = false;

    users.groups.homeserver.gid = config.neo.core.gid;

    users.users.homeserver = {
      uid = config.neo.core.uid;
      group = "homeserver";
      isNormalUser = true;
      home = "/home/homeserver";
      createHome = true;
      extraGroups = ["docker" "wheel"];
      openssh.authorizedKeys.keys = config.neo.core.ssh.authorizedKeys;
      hashedPassword = config.neo.core.hashedLinuxPassword;
    };

    # Allow other neo hosts to use this machine as a remote Nix builder over SSH as homeserver.
    nix.settings.trusted-users = ["homeserver"];

    system.activationScripts.create-volumes = lib.concatStringsSep "\n" (
      lib.map
      (
        dir:
          lib.neo.mkActivationScriptForDir config {
            dirPath = "${dir}";
          }
      )
      [
        config.neo.core.volumes.root
        config.neo.core.volumes.data
        config.neo.core.volumes.appdata
        config.neo.core.volumes.media
        config.neo.core.volumes.documents
      ]
    );

    system.stateVersion = "24.11";
  };
}
