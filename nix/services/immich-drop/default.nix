# Immich-drop service implementation.
{...}: {
  flake.modules.nixos.immich-drop = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.immich-drop;
      immichCfg = config.neo.services.immich;
    in {
      config = mkIf cfg.enabled {
        assertions = lib.neo.requireSecrets "immich-drop" cfg ["sessionSecret"];

        neo.core.secrets.containerEnv.immich-drop = mkIf (cfg.sessionSecret != null) (
          {SESSION_SECRET = cfg.sessionSecret;}
          // optionalAttrs (cfg.apiKey != null) {IMMICH_API_KEY = cfg.apiKey;}
        );

        systemd.services.docker-immich-drop.preStart = lib.neo.mkEnsureDirs config [
          cfg.appdata
        ];

        virtualisation.oci-containers.containers = {
          immich-drop = {
            image = cfg.containers."immich-drop";
            environment = {
              IMMICH_BASE_URL = "${lib.neo.publicUrl config immichCfg}/api";
              IMMICH_ALBUM_NAME = "dead-drop";
              PUBLIC_UPLOAD_PAGE_ENABLED = "false";
              PUBLIC_BASE_URL = lib.neo.publicUrl config cfg;
              CHUNKED_UPLOADS_ENABLED = "true";
              CHUNK_SIZE_MB = "95";
            };
            volumes = [
              "${cfg.appdata}:/data"
            ];
            extraOptions = [
              "--health-cmd=python - <<'PY'\nimport os,urllib.request,sys; url=f\"http://127.0.0.1:{os.getenv('PORT','8080')}/\";\ntry: urllib.request.urlopen(url, timeout=3); sys.exit(0)\nexcept Exception: sys.exit(1)\nPY"
              "--health-interval=30s"
              "--health-timeout=5s"
              "--health-retries=3"
              "--health-start-period=10s"
            ];
            networks = [(lib.neo.docker.serviceNetwork "immich-drop")];
          };
        };
      };
    };
}
