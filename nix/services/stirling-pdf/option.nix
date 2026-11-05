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
              enabled = mkEnableOption "PDF tools in your browser: merge, split, convert, sign and more" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8080;
                description = "Internal port Stirling PDF listens on";
              };
              # SECURITY_ENABLELOGIN; tinyauth already protects the app at the edge.
              enableLogin = mkOption {
                type = types.bool;
                default = false;
                rank = 10;
                description = "Ask for a separate Stirling PDF login on top of the homeserver login. Usually not needed.";
                ui = lib.neo.ui.mkUi {label = "Separate Stirling PDF login";};
              };
              initialLoginUsername = mkOption {
                type = types.str;
                default = "admin";
                rank = 11;
                description = "Username of the first Stirling PDF admin. Only used the very first time the app starts.";
                ui = lib.neo.ui.mkUi {
                  label = "First admin username";
                  visibleWhen = "enableLogin";
                };
              };
              initialLoginPassword = lib.neo.mkSecretOption {
                type = types.str;
                default = "stirling";
                rank = 12;
                description = "Password of the first Stirling PDF admin. Only used the very first time the app starts; change it after you sign in.";
                ui = lib.neo.ui.mkUi {
                  label = "First admin password";
                  visibleWhen = "enableLogin";
                };
                helper = lib.neo.helpers.randomToken;
              };
              defaultLocale = mkOption {
                type = types.str;
                default = "en-US";
                rank = 20;
                # SYSTEM_DEFAULTLOCALE
                description = "Language new users see first, e.g. en-US or de-DE.";
                ui = lib.neo.ui.mkUi {label = "Default language";};
              };
              fileUploadLimit = mkOption {
                type = types.str;
                default = "2000MB";
                rank = 21;
                # SYSTEMFILEUPLOADLIMIT
                description = "Largest file you can upload, e.g. 2000MB or 2GB.";
                ui = lib.neo.ui.mkUi {label = "Upload size limit";};
              };
              googleVisibility = mkOption {
                type = types.bool;
                default = false;
                rank = 22;
                # SYSTEM_GOOGLEVISIBILITY
                description = "Let search engines like Google list this app. Usually best left off.";
                ui = lib.neo.ui.mkUi {label = "Show in search engines";};
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
              license = {
                spdx = "MIT";
                url = "https://github.com/Stirling-Tools/Stirling-PDF/blob/main/LICENSE";
                openSource = false;
                note = "The core is MIT licensed, but the official image also contains features under a closed license.";
              };
              deprecated = {
                reason = "Parts of its official image are closed source, so Neo no longer maintains it.";
                instead = "BentoPDF does the same PDF jobs (edit, merge, split, sign) and is fully open source.";
                alternatives = ["bentopdf"];
              };
              icon = ./icon.svg;
              tagline = "All-in-one PDF tools on your server";
              description = ''
                Merge, split, convert, compress, sign, rotate and watermark PDFs, and make scanned pages searchable, all on your own server without tracking.
              '';
              notes = ''
                tinyauth protects the UI by default; Stirling's own login can be enabled as well. Settings, OCR data and logs persist under appdata; large uploads are allowed.
              '';
              replaces = ["Adobe Acrobat" "iLovePDF"];
              privacy = ["documents"];
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
