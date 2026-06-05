# Beszel agent implementation. Websocket only (no custom LISTEN port), host network, minimal.
{...}: {
  flake.modules.nixos.beszel-agent = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.beszel-agent;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-beszel-agent.preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.core.volumes.appdata}/beszel-agent";
        };
        virtualisation.oci-containers.containers.beszel-agent = {
          image = "henrygd/beszel-agent:latest";
          autoStart = true;
          volumes = [
            "${config.neo.core.volumes.appdata}/beszel-agent:/var/lib/beszel-agent"
            "/var/run/docker.sock:/var/run/docker.sock:ro"
            "/var/run/dbus/system_bus_socket:/var/run/dbus/system_bus_socket:ro"
          ];
          networks = ["host"];
          environment =
            {
              TZ = "Europe/Zurich";
              DISABLE_SSH = "true";
            }
            // (optionalAttrs (cfg.hubUrl != null) {
              HUB_URL = cfg.hubUrl;
            })
            // (optionalAttrs (cfg.key != null) {
              KEY = cfg.key;
            })
            // (optionalAttrs (cfg.token != null) {
              TOKEN = cfg.token;
            });
        };
      };
    };
}
