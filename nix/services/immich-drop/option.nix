# Immich-drop service options.
{...}: {
  flake.modules.nixos.immich-drop-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.immich-drop = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "immich-drop service" {rank = 0;};
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "drop";
              auth.available = false;
            }
            // lib.neo.mkContainerDefinitions {
              "immich-drop" = "ghcr.io/nasogaa/immich-drop:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/immich-drop"
            // lib.neo.mkServiceMeta {
              category = "Media";
              icon = "https://raw.githubusercontent.com/immich-app/immich/main/design/immich-logo.svg";
              description = ''
                Immich Drop is a tiny, zero-login web app for collecting photos/videos from anyone into your Immich server.
                Admin users log in with Immich credentials to create public invite links (always public-by-URL) supporting optional passwords, expiry, one-time use, and target albums (auto-created if needed).
                Guests upload via simple drag-and-drop or file chooser (mobile-friendly) with real-time WebSocket progress, duplicate detection via local SHA-1 cache plus optional Immich bulk-check, chunked uploads for large files, retries, and EXIF timestamp preservation.
                A public uploader page is optional and disabled by default. Privacy-first design: never lists server media; only ephemeral per-session state is shown. Lightweight FastAPI backend + static frontend, containerized.
                Ideal for securely sharing easy upload access with family, friends or guests without needing Immich accounts.
              '';
              projectUrl = "https://github.com/Nasogaa/immich-drop";
              githubUrl = "https://github.com/Nasogaa/immich-drop";
              releaseUrl = "https://github.com/Nasogaa/immich-drop/releases";
            };
        };
        default = {};
        description = "Immich-drop service configuration";
      };
    };
}
