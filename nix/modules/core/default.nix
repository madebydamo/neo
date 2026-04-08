# Core NixOS configuration: openssh, users, activation scripts, timezone, stateVersion.
{...}: {
  flake.modules.nixos.core = {
    config,
    lib,
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

    users.groups.homeserver.gid = config.neo.gid;

    users.users.homeserver = {
      uid = config.neo.uid;
      group = "homeserver";
      isNormalUser = true;
      home = "/home/homeserver";
      createHome = true;
    };

    system.activationScripts.create-volumes = lib.concatStringsSep "\n" (
      lib.map
      (dir:
        lib.neo.mkActivationScriptForDir config {
          dirPath = "${dir}";
        })
      [
        config.neo.volumes.root
        config.neo.volumes.data
        config.neo.volumes.appdata
        config.neo.volumes.media
        config.neo.volumes.documents
      ]
    );

    system.stateVersion = "24.11";
  };
}
