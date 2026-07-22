# VM-specific hardware and virtualisation configuration.
# Stored under devices (not flake.modules.nixos) so it is NOT auto-included.
# Only configurations that explicitly import this module (e.g. QEMU via
# configurations.nixos.homeserver or nixosModules.vm) get these settings.
{
  config,
  self,
  ...
}: {
  devices.vm = {
    config,
    lib,
    ...
  }: {
    # Development SSH key for `just ssh` / tools/development_ed25519.
    # Merged with any keys from settings.toml (listOf concatenates definitions).
    # Not a default on neo.core.ssh.authorizedKeys so real deployments stay clean.
    neo.core.ssh.authorizedKeys = [
      (lib.fileContents (self + "/tools/development_ed25519.pub"))
    ];

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
