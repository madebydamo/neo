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
              enabled = mkEnableOption "Upload links so family and friends can add photos to your Immich without an account" {rank = 0;};

              apiKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Lets guests upload through invite links. Empty means only people logged in with their Immich account can upload. Create the key in Immich under Account Settings, API Keys.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Immich API key";};
              };

              sessionSecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Protects admin logins to this app. Changing it signs everyone out.";
                rank = 20;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Session secret";};
              };
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
              license = {
                spdx = "MIT";
                url = "https://github.com/Nasogaa/immich-drop/blob/master/LICENSE";
              };
              deprecated = {
                reason = "Immich now collects uploads from guests itself, so Neo no longer maintains this app.";
                instead = "In Immich, share an album with a link and turn on Allow public upload: guests add their photos without an account.";
                alternatives = ["immich"];
              };
              icon = ./icon.svg;
              tagline = "Let guests add photos to Immich with a link";
              description = ''
                Create an upload link for a wedding, party or holiday and share it.
                Guests drop their photos and videos into your album without an account, and never see anything else on your server.
              '';
              notes = ''
                Admins log in with their Immich account to create invite links (public by URL) with optional password, expiry, one-time use and a target album (created if needed).
                Uploads are chunked with retries and keep EXIF timestamps; duplicates are detected. A public uploader page without invite is optional and off by default.
              '';
              privacy = ["photos"];
              projectUrl = "https://github.com/Nasogaa/immich-drop";
              githubUrl = "https://github.com/Nasogaa/immich-drop";
              releaseUrl = "https://github.com/Nasogaa/immich-drop/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Immich-drop service configuration";
      };
    };
}
