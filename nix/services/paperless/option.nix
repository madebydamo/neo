# Paperless service options.
{...}: {
  flake.modules.nixos.paperless-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.paperless = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Paperless, a searchable archive for your scanned papers and documents" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8000;
                description = "Internal port for paperless web UI";
              };
              # POSTGRES_PASSWORD of paperless-db and PAPERLESS_DBPASS.
              dbPassword = lib.neo.mkSecretOption {
                type = types.str;
                default = "your_strong_password_here";
                rank = 10;
                description = "Password for Paperless's own database. Press Generate before the first start and keep it: changing it later cuts Paperless off from its data.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {
                  label = "Database password";
                };
              };
              # Required since paperless-ngx 3.0.0 — refuses to start with unset or default 'change-me'.
              # See https://docs.paperless-ngx.com/configuration/ (PAPERLESS_SECRET_KEY) and
              # https://github.com/paperless-ngx/paperless-ngx/issues/13215
              secretKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                # PAPERLESS_SECRET_KEY: Django secret for sessions and signing.
                description = "Secret key Paperless needs to start. Press Generate once and keep it: changing it signs everyone out.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {
                  label = "Secret key";
                };
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "paperless";
              auth.publicPaths = [
                "^/api/"
                "^/static/"
                "^/media/"
                "^/favicon.ico$"
                "^/assets/"
                "^/logo(?:/.*)?$"
                "^/share/"
                "^/fetch/"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              "paperless-redis" = "redis:7-alpine";
              "paperless-db" = "postgres:16";
              "paperless" = "ghcr.io/paperless-ngx/paperless-ngx:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/paperless"
            // lib.neo.mkServiceMeta {
              category = "Files";
              license = {
                spdx = "GPL-3.0";
                url = "https://github.com/paperless-ngx/paperless-ngx/blob/dev/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Scan letters and find them again in seconds";
              description = ''
                Turns paper letters, bills and PDFs into a searchable archive.
                Scan or upload a document: it reads the text, files it by sender and type, and finds it again when you search for any word in it.
              '';
              notes = ''
                Paperless-ngx. Runs OCR on scans and PDFs; documents can come from a watch folder, email accounts or the upload page, and workflows can process them automatically.
              '';
              privacy = ["documents"];
              projectUrl = "https://docs.paperless-ngx.com/";
              githubUrl = "https://github.com/paperless-ngx/paperless-ngx";
              releaseUrl = "https://github.com/paperless-ngx/paperless-ngx/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Paperless service configuration";
      };
    };
}
