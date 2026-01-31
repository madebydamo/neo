{
  config,
  pkgs,
  ...
}:
{
  virtualisation.oci-containers.backend = "docker";
  environment.systemPackages = [ pkgs.docker ];
  boot.initrd.systemd.fido2.enable = false;

  users.users.appuser = {
    uid = config.neo.uid;
    group = "users";
    isNormalUser = true;
  };
}
