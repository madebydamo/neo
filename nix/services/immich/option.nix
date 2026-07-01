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
              enabled = mkEnableOption "immich service" {rank = 0;};
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "immich";
              auth.publicPaths = [
                "^/share/"
                "^/.well-known/immich"
                "^/api/"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              "immich-server" = "ghcr.io/immich-app/immich-server:release";
              "immich-machine-learning" = "ghcr.io/immich-app/immich-machine-learning:release";
              "immich-redis" = "docker.io/valkey/valkey:8@sha256:81db6d39e1bba3b3ff32bd3a1b19a6d69690f94a3954ec131277b9a26b95b3aa";
              "immich-database" = "ghcr.io/immich-app/postgres:14-vectorchord0.4.3-pgvectors0.2.0@sha256:bcf63357191b76a916ae5eb93464d65c07511da41e3bf7a8416db519b40b1c23";
            }
            // lib.neo.mkServiceMeta {
              icon = "https://raw.githubusercontent.com/immich-app/immich/main/design/immich-logo.svg";
              description = ''
                Immich is a high-performance self-hosted photo and video management solution and a powerful open-source alternative to Google Photos.
                It enables you to back up, organize, search, and manage your photos and videos on your own server with ease, featuring automatic mobile backups via official apps for Android and iOS.
                Advanced capabilities include machine learning for facial recognition, object detection, semantic search, a map view, memories, sharing, and support for raw files and Live Photos.
                Immich delivers a polished web UI and companion mobile apps, giving you complete privacy and control over your media library without any third-party cloud services.
              '';
              projectUrl = "https://immich.app/";
              githubUrl = "https://github.com/immich-app/immich";
              releaseUrl = "https://github.com/immich-app/immich/releases";
            };
        };
        default = {};
        description = "Immich service configuration";
      };
    };
}
