{ lib, ... }:
{
  users.allowNoPasswordLogin = true;
  users.mutableUsers = false;
  services.dbus.enable = lib.mkForce false;
  # Minimal boot and filesystem config for hardware/VM deployment
  # boot.loader.grub.device = "/dev/sda";
  # fileSystems."/".device = "/dev/sda1";
  # fileSystems."/".fsType = "ext4";
}
