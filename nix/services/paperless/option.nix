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
              enabled = mkEnableOption "paperless document management service" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8000;
                description = "Internal port for paperless web UI";
              };
              dbPassword = mkOption {
                type = types.str;
                default = "your_strong_password_here";
                rank = 10;
                description = "Password for internal docker connection";
                helper = lib.neo.helpers.randomToken;
              };
              # Required since paperless-ngx 3.0.0 — refuses to start with unset or default 'change-me'.
              # See https://docs.paperless-ngx.com/configuration/ (PAPERLESS_SECRET_KEY) and
              # https://github.com/paperless-ngx/paperless-ngx/issues/13215
              secretKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = ''
                  PAPERLESS_SECRET_KEY: Django secret used for sessions and signing.
                  Required by paperless-ngx 3.0+ (startup fails if unset or the default "change-me").
                  Generate once with the helper and keep stable; rotating invalidates sessions/tokens.
                '';
                helper = lib.neo.helpers.randomToken;
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
              "paperless-redis" = "redis:7";
              "paperless-db" = "postgres:16";
              "paperless" = "ghcr.io/paperless-ngx/paperless-ngx:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/paperless"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/paperless-ngx.svg";
              description = ''
                Paperless-ngx is a community-supported document management system that transforms your physical documents into a searchable online archive.
                It performs OCR on scans and PDFs to enable powerful full-text search, tagging, and organization by correspondents and document types.
                Automate document ingestion from watch folders, email accounts, or the upload interface, and define workflows for processing.
                With its modern web UI, it helps you go paperless while keeping everything organized, versioned, and easily accessible.
              '';
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
