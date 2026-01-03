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

  # Minimal boot and filesystem config for hardware/VM deployment
  # boot.loader.grub.device = "/dev/sda";
  # fileSystems."/".device = "/dev/sda1";
  # fileSystems."/".fsType = "ext4";
}

