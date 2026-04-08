# Immich-drop service implementation.
{...}: {
  flake.modules.nixos.immich-drop = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.immich-drop;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-immich-drop.preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${config.neo.volumes.appdata}/immich-drop";
          })
        ];

        virtualisation.oci-containers.containers = {
          immich-drop = {
            image = "ghcr.io/nasogaa/immich-drop:latest";
            autoStart = true;
            environment = {
              IMMICH_BASE_URL = "https://immich.damo4mf20.ch/api";
              IMMICH_API_KEY = "N0iWGGfNrozgwBfPuTLQYAAjYfv6rxJMcDN6Xfo8c";
              IMMICH_ALBUM_NAME = "dead-drop";
              PUBLIC_UPLOAD_PAGE_ENABLED = "false";
              PUBLIC_BASE_URL = "https://drop.damo4mf20.ch";
              CHUNKED_UPLOADS_ENABLED = "true";
              CHUNK_SIZE_MB = "95";
              SESSION_SECRET = "SET-A-STRONG-RANDOM-VALUE";
            };
            volumes = [
              "${config.neo.volumes.appdata}/immich-drop:/data"
            ];
            extraOptions = [
              "--network=internal"
              "--health-cmd=python - <<'PY'\nimport os,urllib.request,sys; url=f\"http://127.0.0.1:{os.getenv('PORT','8080')}/\";\ntry: urllib.request.urlopen(url, timeout=3); sys.exit(0)\nexcept Exception: sys.exit(1)\nPY"
              "--health-interval=30s"
              "--health-timeout=5s"
              "--health-retries=3"
              "--health-start-period=10s"
            ];
          };
        };
      };
    };
}
