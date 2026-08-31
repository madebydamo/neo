# Dufs service implementation (single container).
# Web UI behind tinyauth via SWAG; /__dufs__/health on publicPaths.
# When password is set, dufs HTTP Basic is required for WebDAV (SWAG skips tinyauth
# for DAV methods and Authorization-bearing requests and injects Basic for the UI).
{...}: {
  flake.modules.nixos.dufs = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.dufs;
      appdata = "${config.neo.core.volumes.appdata}/dufs";
      dataDir = "${appdata}/data";
      webdavAuth =
        (cfg.password or null)
        != null
        && cfg.password != "";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-dufs.preStart = lib.neo.mkEnsureDirs config [
          appdata
          dataDir
        ];

        virtualisation.oci-containers.containers.dufs = {
          image = cfg.containers.dufs;
          autoStart = true;
          user = "${toString config.neo.core.uid}:${toString config.neo.core.gid}";
          environment =
            {
              TZ = config.neo.core.timeZone;
              DUFS_SERVE_PATH = "/data";
              DUFS_BIND = "0.0.0.0";
              DUFS_PORT = toString cfg.port;
              DUFS_ALLOW_ALL = "true";
              DUFS_ENABLE_CORS = "true";
              DUFS_HIDDEN = ".git,.DS_Store";
            }
            // optionalAttrs webdavAuth {
              DUFS_AUTH = "${cfg.username}:${cfg.password}@/:rw";
            };
          volumes =
            [
              "${dataDir}:/data"
              "${config.neo.core.volumes.media}:/data/Media"
              "${config.neo.core.volumes.documents}:/data/Documents"
            ]
            ++ lib.neo.toOciBindMounts cfg.additionalMountPoints;
          networks = ["internal"];
        };
      };
    };
}
