# VM-specific hardware and virtualisation configuration.
# Stored under devices (not flake.modules.nixos) so it is NOT auto-included.
{...}: {
  devices.vm = {
    config,
    lib,
    ...
  }: {
    users.allowNoPasswordLogin = true;
    users.mutableUsers = false;
    boot.loader.grub.device = "/dev/vda";
    fileSystems."/".device = "/dev/vda1";
    fileSystems."/".fsType = "ext4";
    virtualisation = {
      diskSize = 1024000;
      docker.enable = true;
      oci-containers.backend = "docker";
    };
  };
}
