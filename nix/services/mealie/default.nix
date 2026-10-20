# Mealie service implementation (single container, SQLite).
# UI behind tinyauth via SWAG. /api/ is on publicPaths for tokens, apps, and /api/app/about.
# proxy.conf already sets Upgrade/Connection. Do not re-set them.
{...}: {
  flake.modules.nixos.mealie = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.mealie;
      appdata = "${config.neo.core.volumes.appdata}/mealie";
      domain = config.neo.services.swag.domain or null;
      publicHost =
        if (cfg.subdomain or "") == "" || domain == null || domain == ""
        then ""
        else "${cfg.subdomain}.${domain}";
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = publicHost != "";
            message = "neo.services.mealie: subdomain and services.swag.domain must be set when enabled.";
          }
        ];

        # entry.sh chowns /app (including this mount) to PUID:PGID on start.
        systemd.services.docker-mealie.preStart = lib.neo.mkEnsureDirs config [appdata];

        virtualisation.oci-containers.containers.mealie = {
          image = cfg.containers.mealie;
          autoStart = true;
          environment = {
            PUID = toString config.neo.core.uid;
            PGID = toString config.neo.core.gid;
            TZ = config.neo.core.timeZone;
            BASE_URL = "https://${publicHost}";
            ALLOW_SIGNUP = boolToString cfg.allowSignup;
            API_PORT = toString cfg.port;
          };
          volumes = [
            "${appdata}:/app/data"
          ];
          # Upstream recommends a memory cap. CPython otherwise holds onto a large
          # idle allocation on hosts with a lot of RAM.
          extraOptions = [
            "--memory=1000m"
          ];
          networks = ["internal"];
        };
      };
    };
}
