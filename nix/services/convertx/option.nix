# ConvertX service options.
# Image and settings from https://github.com/C4illin/ConvertX (ghcr.io/c4illin/convertx).
# Edge login is tinyauth. ConvertX also has its own accounts unless allowUnauthenticated.
# The sign-in key is JWT_SECRET: a new one on every start would sign everyone out.
# HTTP_ALLOWED stays off so session cookies keep the Secure flag behind HTTPS.
{...}: {
  flake.modules.nixos.convertx-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.convertx = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "convert documents, photos, video and audio between more than a thousand formats" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3000;
                description = "Internal port ConvertX listens on";
              };
              # JWT_SECRET. Anonymous sessions use it too.
              sessionSecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "Key that signs sign-in cookies. Generate one and keep it. Changing it signs everyone out.";
                ui = lib.neo.ui.mkUi {label = "Sign-in key";};
                helper = lib.neo.helpers.randomToken;
              };
              # ACCOUNT_REGISTRATION. The welcome page can always create the first account.
              allowRegistration = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = "Let people who can open the app create their own account. The first account can always be created on the welcome page.";
                ui = lib.neo.ui.mkUi {label = "Allow more accounts";};
              };
              # ALLOW_UNAUTHENTICATED. Still sits behind tinyauth unless Require login is off.
              allowUnauthenticated = mkOption {
                type = types.bool;
                default = false;
                rank = 21;
                description = "Skip the app's own account and let anyone who can open the page convert files. Leave this off unless Require login is on.";
                ui = lib.neo.ui.mkUi {label = "Skip the app's own login";};
              };
              # AUTO_DELETE_EVERY_N_HOURS. 0 disables the sweep.
              keepHours = mkOption {
                type = types.ints.unsigned;
                default = 24;
                rank = 30;
                description = "How long uploaded and converted files are kept, in hours. 0 keeps them until you delete them.";
                ui = lib.neo.ui.mkUi {label = "Keep files for (hours)";};
              };
              # HIDE_HISTORY.
              hideHistory = mkOption {
                type = types.bool;
                default = false;
                rank = 31;
                description = "Hide the list of past conversions. Files are still removed on the usual schedule.";
                ui = lib.neo.ui.mkUi {label = "Hide conversion history";};
              };
              # LANGUAGE, a BCP 47 tag. Dates only; the app UI stays English.
              language = mkOption {
                type = types.str;
                default = "en";
                rank = 32;
                description = "How dates are written, for example en or de.";
                ui = lib.neo.ui.mkUi {label = "Date language";};
              };
              # MAX_CONVERT_PROCESS. 0 means unlimited.
              maxConversions = mkOption {
                type = types.ints.unsigned;
                default = 0;
                rank = 40;
                description = "How many conversions can run at the same time. 0 means no limit.";
                ui = lib.neo.ui.mkUi {
                  label = "Conversions at once";
                  group = lib.neo.ui.groups.advanced;
                };
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "convertx";
              # GET /healthcheck is unauthenticated upstream.
              auth.publicPaths = ["^/healthcheck$"];
            }
            // lib.neo.mkContainerDefinitions {
              convertx = "ghcr.io/c4illin/convertx:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/convertx"
            // lib.neo.mkServiceMeta {
              ownLogin =
                if config.neo.services.convertx.allowUnauthenticated or false
                then null
                else "ConvertX accounts";
              category = "Files";
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/C4illin/ConvertX/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Convert almost any file, on your server";
              description = ''
                Turn documents, photos, video, audio and e-books into another format.
                More than a thousand formats, including Office files and video, converted on your server instead of a website.
              '';
              notes = ''
                The homeserver login is in front. The first time you open the app, the welcome page asks you to create an account. Later visits ask for that account too, unless Skip the app's own login is on.
                Uploaded and converted files are removed after Keep files for. Clearing app data removes accounts and leftover files.
                Generate the sign-in key once and leave it. Changing it signs everyone out.
                The download is large because office, video and image converters are included. The first start can take a minute.
              '';
              replaces = ["CloudConvert" "Convertio" "Zamzar"];
              privacy = ["documents" "files"];
              projectUrl = "https://github.com/C4illin/ConvertX";
              githubUrl = "https://github.com/C4illin/ConvertX";
              releaseUrl = "https://github.com/C4illin/ConvertX/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "ConvertX service configuration";
      };
    };
}
