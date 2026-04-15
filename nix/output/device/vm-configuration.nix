# VM-specific hardware and virtualisation configuration.
# Stored under devices (not flake.modules.nixos) so it is NOT auto-included.
{config, ...}: {
  devices.vm = {
    config,
    lib,
    ...
  }: {
    users.allowNoPasswordLogin = true;
    users.mutableUsers = false;
    # boot.loader.grub.device = "/dev/vda";
    fileSystems."/".device = lib.mkDefault "/dev/vda1";
    fileSystems."/".fsType = lib.mkDefault "ext4";
    virtualisation = {
      diskSize = 1024000;
      docker.enable = true;
      oci-containers.backend = "docker";
    };
  };
  flake.nixosModules.vm = config.devices.vm;
}
