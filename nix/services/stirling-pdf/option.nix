# Stirling PDF service options.
# Deployment volumes/env from https://docs.stirlingpdf.com/Production-Deployment-Guide
# Edge auth: tinyauth on by default; Stirling's own login is off by default.
{...}: {
  flake.modules.nixos.stirling-pdf-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.stirling-pdf = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Stirling PDF document toolkit service" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8080;
                description = "Internal port Stirling PDF listens on";
              };
              enableLogin = mkOption {
                type = types.bool;
                default = false;
                rank = 10;
                description = "Enable Stirling PDF built-in user authentication (SECURITY_ENABLELOGIN). Off by default; use tinyauth at the edge instead.";
              };
              initialLoginUsername = mkOption {
                type = types.str;
                default = "admin";
                rank = 11;
                description = "Initial admin username (only applied on first startup before the DB exists)";
              };
              initialLoginPassword = mkOption {
                type = types.str;
                default = "stirling";
                rank = 12;
                description = "Initial admin password (change after first login; only applied on first startup)";
                helper = lib.neo.helpers.randomToken;
              };
              defaultLocale = mkOption {
                type = types.str;
                default = "en-US";
                rank = 20;
                description = "Default UI locale for new users (SYSTEM_DEFAULTLOCALE)";
              };
              fileUploadLimit = mkOption {
                type = types.str;
                default = "2000MB";
                rank = 21;
                description = "Max upload size (SYSTEMFILEUPLOADLIMIT, e.g. 2000MB or 2GB)";
              };
              googleVisibility = mkOption {
                type = types.bool;
                default = false;
                rank = 22;
                description = "Allow search engines to index the instance (SYSTEM_GOOGLEVISIBILITY)";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "stirling";
              auth = {
                enabled = true;
                # Health check is unauthenticated upstream; keep it public for probes.
                publicPaths = ["^/api/v1/info/status$"];
              };
            }
            // lib.neo.mkContainerDefinitions {
              "stirling-pdf" = "stirlingtools/stirling-pdf:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/stirling-pdf"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/stirling-pdf.svg";
              description = ''
                Stirling PDF is a locally hosted web application for performing PDF operations with no tracking or data sharing.
                Merge, split, convert, OCR, compress, sign, rotate, watermark, and dozens more tools run entirely on your server.
                Neo protects the UI with tinyauth by default; optional built-in login can be enabled. Configs, OCR data, and logs persist under appdata behind HTTPS with large upload limits.
              '';
              projectUrl = "https://docs.stirlingpdf.com/";
              githubUrl = "https://github.com/Stirling-Tools/Stirling-PDF";
              releaseUrl = "https://github.com/Stirling-Tools/Stirling-PDF/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Stirling PDF service configuration";
      };
    };
}
