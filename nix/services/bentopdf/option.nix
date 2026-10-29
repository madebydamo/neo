# BentoPDF service options.
# Static, client-side PDF toolkit (files never leave the browser); no appdata.
# Self-hosting: https://github.com/alam00000/bentopdf#-self-hosting
{...}: {
  flake.modules.nixos.bentopdf-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.bentopdf = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "BentoPDF privacy-first PDF toolkit" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8080;
                description = "Internal port BentoPDF's nginx listens on";
              };
              disabledTools = mkOption {
                type = types.listOf types.str;
                default = [];
                rank = 10;
                description = "Tool IDs to hide (config.json disabledTools). A tool's ID is its page URL without .html, e.g. edit-pdf, sign-pdf, encrypt-pdf.";
              };
              editorDisabledCategories = mkOption {
                type = types.listOf types.str;
                default = [];
                rank = 11;
                description = "PDF editor feature categories to hide (config.json editorDisabledCategories), e.g. redaction.";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "bentopdf";
              auth.enabled = true;
            }
            // lib.neo.mkContainerDefinitions {
              bentopdf = "ghcr.io/alam00000/bentopdf-simple:latest";
            }
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = ./icon.svg;
              description = ''
                BentoPDF is a privacy-first PDF toolkit that runs entirely in the browser: files are processed client-side and never uploaded.
                Merge, split, compress, convert, edit, sign, OCR, redact and 50+ more tools, including Office conversion via LibreOffice WASM.
                Neo serves the self-hosted "simple" build behind tinyauth; individual tools can be hidden without rebuilding.
              '';
              projectUrl = "https://www.bentopdf.com/";
              githubUrl = "https://github.com/alam00000/bentopdf";
              releaseUrl = "https://github.com/alam00000/bentopdf/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "BentoPDF service configuration";
      };
    };
}
