{
  config,
  lib,
  ...
}: {
  users.allowNoPasswordLogin = true;
  users.mutableUsers = false;
  # Minimal boot and filesystem config for hardware/VM deployment
  boot.loader.grub.device = "/dev/vda";
  fileSystems."/".device = "/dev/vda1";
  fileSystems."/".fsType = "ext4";
  virtualisation = {
    diskSize = 1024000; # 1TB
    docker.enable = true;
    oci-containers.backend = "docker";
  };
}
