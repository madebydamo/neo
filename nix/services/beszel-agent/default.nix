# Beszel agent implementation. Websocket only (no custom LISTEN port), host network, minimal.
{...}: {
  flake.modules.nixos.beszel-agent = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.beszel-agent;
    in {
      config = mkIf cfg.enabled {
        virtualisation.oci-containers.containers.beszel-agent = {
          image = "henrygd/beszel-agent:latest";
          autoStart = true;
          volumes = [
            "/var/run/docker.sock:/var/run/docker.sock:ro"
          ];
          extraOptions = [
            "--network=host"
          ];
          environment = {
            TZ = "Europe/Zurich";
          } // (optionalAttrs (cfg.hubUrl != null) {
            HUB_URL = cfg.hubUrl;
          }) // (optionalAttrs (cfg.key != null) {
            KEY = cfg.key;
          }) // (optionalAttrs (cfg.token != null) {
            TOKEN = cfg.token;
          });
        };
      };
    };
}
