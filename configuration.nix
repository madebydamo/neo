{ pkgs, ... }:

{
  environment.systemPackages = [ pkgs.docker ];

  systemd.services.hello-docker = {
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${pkgs.docker}/bin/docker run --rm hello-world";
    };
  };

  system.stateVersion = "24.11";
}