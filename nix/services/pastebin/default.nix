# Pastebin service implementation.
{...}: {
  flake.modules.nixos.pastebin = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.pastebin;
      appdata = "${config.neo.volumes.appdata}/pastebin";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-pastebin.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = appdata;
        };

        virtualisation.oci-containers.containers.pastebin = {
          image = "wantguns/bin";
          autoStart = true;
          environment = {
            BIN_PORT = toString cfg.port;
            BIN_LIMITS = ''{form="16 MiB"}'';
            BIN_CLIENT_DESC = "placeholder";
          };
          volumes = [
            "${appdata}:/upload"
          ];
          extraOptions = [
            "--network=internal"
          ];
        };
      };
    };
}
