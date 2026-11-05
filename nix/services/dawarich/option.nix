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
              enabled = mkEnableOption "Your own location history: a map of where you have been, like Google Timeline" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3000;
                description = "Internal port Dawarich listens on (container port 3000)";
              };
              # SECRET_KEY_BASE: signs sessions and derives encryption keys. Changing it signs
              # everyone out and can make encrypted data unreadable. The app refuses CHANGE_ME.
              secretKeyBase = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "Secret that secures logins and stored data. Click Generate once and never change it.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Secret key";};
              };
              # POSTGRES_PASSWORD / DATABASE_PASSWORD of the internal PostGIS container.
              dbPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Password for Dawarich's own database. Press Generate before the first start and keep it: changing it later cuts Dawarich off from its data.";
                ui = lib.neo.ui.mkUi {
                  label = "Database password";
                };
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
              "dawarich-redis" = "redis:8-alpine";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/dawarich"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/Freika/dawarich/blob/master/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Your own location history, like Google Timeline";
              description = ''
                A private map of where you have been: trips, places you visited and statistics per month and year.
                Your phone sends its location to your own server instead of to Google.
              '';
              notes = ''
                Runs the upstream stack: the Rails app, a Sidekiq worker, PostGIS for the points and Redis for jobs and live updates.
                Phone apps and trackers (OwnTracks, Overland, Traccar, the Dawarich apps) post to /api/ with a Dawarich API key, so that path skips tinyauth. The web UI stays behind tinyauth, then Dawarich's own login.
                The image tag is pinned. Read the release notes before upgrading, and change the app and Sidekiq image together.
                Create your account with Sign up on the Dawarich login page.
              '';
              replaces = ["Google Maps Timeline"];
              privacy = ["location"];
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
