{ config, lib, ... }:
{
  imports = [
    ./options.nix
    ./services
  ];

  services.openssh = {
    enable = true;
    settings.PermitRootLogin = "yes";
  };

  users.users.root.openssh.authorizedKeys.keys = config.neo.ssh.authorizedKeys;

  users.groups.homeserver.gid = config.neo.gid;

  users.users.homeserver = {
    uid = config.neo.uid;
    group = "homeserver";
    isNormalUser = true;
    home = "/home/homeserver";
    createHome = true;
  };

  system.stateVersion = "24.11";
}
