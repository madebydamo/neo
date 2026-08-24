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

        systemd.services.rustical-provision = mkIf ssoEnabled {
          description = "Provision RustiCal principals for tinyauth SSO";
          after = ["docker-rustical.service"];
          requires = ["docker-rustical.service"];
          wantedBy = ["multi-user.target"];
          path = [pkgs.docker pkgs.coreutils];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };
          script = let
            password = lib.escapeShellArg cfg.ssoPassword;
            idList = lib.concatMapStringsSep " " lib.escapeShellArg principalIds;
          in ''
            set -euo pipefail
            rustical() {
              docker exec rustical /usr/local/bin/rustical "$@"
            }
            echo "Waiting for rustical health"
            for _ in $(seq 1 60); do
              if rustical health >/dev/null 2>&1; then
                break
              fi
              sleep 2
            done
            rustical health >/dev/null
            for id in ${idList}; do
              rustical principals create "$id" || true
              printf '%s\n' ${password} | docker exec -i rustical /usr/local/bin/rustical principals edit "$id" --password
            done
          '';
        };
        systemd.services.docker-rustical.wants = mkIf ssoEnabled ["rustical-provision.service"];
      };
    };
}
