# Paperless service options.
{...}: {
  flake.modules.nixos.paperless-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.paperless = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "paperless document management service";
              port = mkOption {
                type = types.port;
                default = 8000;
                description = "Internal port for paperless web UI";
              };
              dbPassword = mkOption {
                type = types.str;
                default = "your_strong_password_here";
                description = "Password for internal docker connection";
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
            // lib.neo.mkServiceMeta {
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
            };
        };
        default = {};
        description = "Paperless service configuration";
      };
    };
}
