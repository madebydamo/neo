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

  system.stateVersion = "24.11";
}
