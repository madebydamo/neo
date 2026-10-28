# Dawarich (self-hosted location history) service options.
# Production layout from upstream docker/docker-compose.yml (1.15.3):
# app + Sidekiq + PostGIS + Redis. UI behind tinyauth; /api/ bypasses it
# so phone apps and OwnTracks/Overland/Traccar can use Dawarich's API key.
{...}: {
  flake.modules.nixos.dawarich-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.dawarich = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Dawarich location history" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3000;
                description = "Internal port Dawarich listens on (container port 3000)";
              };
              secretKeyBase = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  SECRET_KEY_BASE: long random secret used to sign sessions and derive encryption keys.
                  Generate once and keep it. Changing it signs everyone out and can make encrypted data unreadable.
                  The app refuses to start with the placeholder CHANGE_ME.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              dbPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Password for the internal PostGIS container (POSTGRES_PASSWORD / DATABASE_PASSWORD)";
                helper = lib.neo.helpers.randomToken;
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "dawarich";
              auth.publicPaths = [
                # Phone apps, OwnTracks, Overland, Traccar, and /api/v1/health.
                # Dawarich authenticates these with its own API key or session.
                "^/api/"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              # Pinned: upstream says to read release notes before upgrading.
              # Keep dawarich and dawarich-sidekiq on the same tag.
              dawarich = "freikin/dawarich:1.15.3";
              "dawarich-sidekiq" = "freikin/dawarich:1.15.3";
              "dawarich-db" = "postgis/postgis:17-3.5-alpine";
              "dawarich-redis" = "redis:7.4-alpine";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/dawarich"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.svg;
              description = ''
                Dawarich is a self-hosted location history (Google Timeline alternative): a map of where you have been, trips, visits, and stats.
                Neo runs the upstream Docker stack: the Rails app, a Sidekiq worker, PostGIS for the points, and Redis for jobs and live updates, on the internal network behind SWAG and tinyauth.
                Phone apps and trackers (OwnTracks, Overland, Traccar, the Dawarich apps) post to /api/ with a Dawarich API key, so that path bypasses tinyauth. The web UI stays behind tinyauth, then Dawarich's own login.
                The image tag is pinned. Dawarich asks you to read the release notes before upgrading. Change the app and Sidekiq image together.
                The first boot creates an admin user demo@dawarich.app with password safepassword. Change that password after you sign in.
              '';
              projectUrl = "https://dawarich.app/docs";
              githubUrl = "https://github.com/Freika/dawarich";
              releaseUrl = "https://github.com/Freika/dawarich/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Dawarich service configuration";
      };
    };
}
