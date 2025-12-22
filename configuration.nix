{ pkgs, lib, ... }:

{
  environment.systemPackages = [ pkgs.docker ];

  boot.initrd.systemd.fido2.enable = false;

  users.allowNoPasswordLogin = true;

  services.dbus.enable = lib.mkForce false;

  systemd.services.hello-docker = {
    description = "Launch hello-world container";
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${pkgs.docker}/bin/docker run --rm hello-world";
    };
  };

  systemd.services.idle = {
    description = "Idle service to keep container running";
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "simple";
      ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
    };
  };

  networking.firewall.enable = false;
  users.mutableUsers = false;

  system.stateVersion = "24.11";
}