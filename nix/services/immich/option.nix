# Immich service options.
{...}: {
  flake.modules.nixos.immich-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.immich = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Your own Google Photos: back up, browse and share your photos and videos" {rank = 0;};
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "immich";
              auth.publicPaths = [
                "^/share/"
                "^/.well-known/immich"
                "^/api/"
                "^/_app/immutable/"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              "immich-server" = "ghcr.io/immich-app/immich-server:release";
              "immich-machine-learning" = "ghcr.io/immich-app/immich-machine-learning:release";
              "immich-redis" = "docker.io/valkey/valkey:9-alpine";
              "immich-database" = "ghcr.io/immich-app/postgres:14-vectorchord0.4.3-pgvectors0.2.0@sha256:bcf63357191b76a916ae5eb93464d65c07511da41e3bf7a8416db519b40b1c23";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/immich"
            // lib.neo.mkServiceMeta {
              category = "Media";
              icon = ./icon.svg;
              tagline = "Your own Google Photos";
              description = ''
                Your phones back up photos and videos automatically. Find pictures by face, place or anything you type, look back with memories and a map, and share albums with family.
                There are apps for Android and iPhone.
              '';
              notes = ''
                Face recognition and smart search run on this server with machine learning; nothing goes to a cloud service.
                Supports raw files and Live Photos.
              '';
              replaces = ["Google Photos" "iCloud Photos"];
              privacy = ["photos"];
              projectUrl = "https://immich.app/";
              githubUrl = "https://github.com/immich-app/immich";
              releaseUrl = "https://github.com/immich-app/immich/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Immich service configuration";
      };
    };
}
