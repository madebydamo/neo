{ pkgs, lib, ... }:

{
  imports = [
    ./options.nix
    ./services
  ];

  virtualisation.oci-containers.backend = "docker";

  environment.systemPackages = [ pkgs.docker ];

  boot.initrd.systemd.fido2.enable = false;

  users.allowNoPasswordLogin = true;

  services.dbus.enable = lib.mkForce false;

  users.mutableUsers = false;

  system.stateVersion = "24.11";
}