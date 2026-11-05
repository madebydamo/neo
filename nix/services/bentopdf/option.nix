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
              enabled = mkEnableOption "PDF tools in your browser: merge, split, sign and more, without uploading your files" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8080;
                description = "Internal port BentoPDF's nginx listens on";
              };
              # config.json disabledTools. A tool's ID is its page URL without .html.
              disabledTools = mkOption {
                type = types.listOf types.str;
                default = [];
                rank = 10;
                description = "Tools to hide from the menu, e.g. sign-pdf or encrypt-pdf (the tool's web address without .html). Empty: all tools are shown.";
                ui = lib.neo.ui.mkUi {label = "Hidden tools";};
              };
              # config.json editorDisabledCategories.
              editorDisabledCategories = mkOption {
                type = types.listOf types.str;
                default = [];
                rank = 11;
                description = "Feature groups to hide in the PDF editor, e.g. redaction. Empty: the editor shows everything.";
                ui = lib.neo.ui.mkUi {label = "Hidden editor features";};
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
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/alam00000/bentopdf/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Edit, merge and sign PDFs right in your browser";
              description = ''
                More than 50 PDF tools: merge, split, compress, sign, fill in forms, redact and convert to and from Office files.
                Your files are processed in your own browser and are never uploaded anywhere, not even to this server.
              '';
              notes = ''
                Neo serves the self-hosted "simple" build behind tinyauth. Office conversion runs LibreOffice compiled to WebAssembly in the browser.
                Individual tools can be hidden without rebuilding (disabledTools).
              '';
              replaces = ["Adobe Acrobat" "iLovePDF" "Smallpdf"];
              privacy = ["documents"];
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
