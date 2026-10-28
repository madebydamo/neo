# RustiCal service implementation (single SQLite-backed container).
# Web UI behind tinyauth via SWAG; /ping and DAV paths on publicPaths.
# Root PROPFIND/OPTIONS/REPORT skip tinyauth in SWAG (not via publicPaths).
# When ssoPassword is set, provision principals for tinyauth users so SWAG can
# complete RustiCal frontend login without a second password form.
{...}: {
  flake.modules.nixos.rustical = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.rustical;
      appdata = "${config.neo.core.volumes.appdata}/rustical";
      tinyauthCfg = config.neo.services.tinyauth or {};
      principalIds =
        lib.filter (
          id:
            id
            != ""
            && !(lib.hasInfix "$" id)
        ) (
          map (u: lib.head (lib.splitString ":" u)) (tinyauthCfg.users or [])
        );
      ssoEnabled =
        cfg.auth.enabled
        && (tinyauthCfg.enabled or false)
        && (cfg.ssoPassword or null)
        != null
        && cfg.ssoPassword != ""
        && principalIds != [];
      passwordFile = config.neo.core.secrets.templates."rustical-sso-password".path;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-rustical.preStart = lib.neo.mkEnsureDirs config [appdata];

        virtualisation.oci-containers.containers.rustical = {
          image = cfg.containers.rustical;
          autoStart = true;
          environment = {
            TZ = config.neo.core.timeZone;
            # Default image bind is [::]:4000; force IPv4 so Docker DNS/health work.
            RUSTICAL_HTTP__BIND = "0.0.0.0:${toString cfg.port}";
            RUSTICAL_DATA_STORE__SQLITE__DB_URL = "/var/lib/rustical/db.sqlite3";
          };
          volumes = [
            "${appdata}:/var/lib/rustical"
          ];
          networks = ["internal"];
        };

        # The password is a secret: provisioning reads it from a rendered file
        # (one line, as the CLI prompt expects).
        neo.core.secrets.templates."rustical-sso-password" = mkIf ssoEnabled {
          content = "${cfg.ssoPassword}\n";
          restartUnits = ["rustical-provision.service"];
        };

        # Setup unit (lib.neo.mkSetupService). Always defined because the
        # option lists it in systemdUnits; without SSO it has nothing to do
        # and finishes at once. Retries every 10s until rustical is healthy.
        systemd.services.rustical-provision = lib.neo.mkSetupService {
          inherit pkgs;
          name = "rustical-provision";
          description = "Provision RustiCal principals for tinyauth SSO";
          containers = ["rustical"];
          path = [pkgs.docker pkgs.coreutils];
          retryInterval = 10;
          script =
            if ssoEnabled
            then let
              idList = lib.concatMapStringsSep " " lib.escapeShellArg principalIds;
            in ''
              set -euo pipefail
              rustical() {
                docker exec rustical /usr/local/bin/rustical "$@"
              }
              rustical health >/dev/null
              for id in ${idList}; do
                rustical principals create "$id" || true
                docker exec -i rustical /usr/local/bin/rustical principals edit "$id" --password <${passwordFile}
              done
            ''
            else ''
              echo "rustical SSO is off (no ssoPassword, tinyauth or users): nothing to provision"
            '';
        };
      };
    };
}
