# RustiCal (CalDAV/CardDAV) service options.
# Web UI protected with tinyauth; DAV/discovery/health bypass via publicPaths.
# PROPFIND/OPTIONS/REPORT on / skip tinyauth in SWAG (308 to well-known CalDAV);
{...}: {
  flake.modules.nixos.rustical-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.rustical = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "your own calendar and contacts server that syncs with your phone and computer" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 4000;
                description = "Internal port RustiCal listens on";
              };
              # Shared secret SWAG uses to complete the RustiCal frontend login after tinyauth.
              # When set (and tinyauth is on), Neo provisions a principal per tinyauth user.
              # Not a CalDAV password: native clients still use app tokens. Keep it stable.
              ssoPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "Generate once to skip RustiCal's second login screen after you sign in to the homeserver. Calendar apps on phones still use their own app passwords.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Single sign-on secret";};
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "rustical";
              auth = {
                enabled = true;
                publicPaths = [
                  # Health probe for smoke tests and monitors (no session cookie).
                  "^/ping$"
                  # CalDAV / CardDAV clients use HTTP Basic + app tokens, not tinyauth cookies.
                  "^/caldav"
                  "^/caldav-compat"
                  "^/carddav"
                  "^/\\.well-known/caldav"
                  "^/\\.well-known/carddav"
                  "^/remote\\.php/dav"
                  "^/index\\.php/login/v2"
                  "^/push_subscription"
                ];
              };
            }
            // lib.neo.mkContainerDefinitions {
              rustical = "ghcr.io/lennart-k/rustical:latest";
              extraUnits = ["rustical-provision"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/rustical"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/lennart-k/rustical/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Calendars, to-dos and contacts that stay at home";
              description = ''
                Keeps your calendars, to-do lists and contacts on your own server.
                Phones and computers sync with it like with Google or iCloud, using the apps you already have: iPhone, Android (with DAVx5), Mac, Thunderbird and more.
              '';
              notes = ''
                CalDAV and CardDAV server storing everything in one SQLite database. Supports WebDAV Push, a Nextcloud-compatible login flow and Apple configuration profiles.
                The web frontend is behind tinyauth; when ssoPassword is set, you are signed into RustiCal as the tinyauth user, so there is no second login.
                CalDAV/CardDAV, well-known discovery and /ping skip tinyauth so calendar apps can use RustiCal app tokens. PROPFIND, OPTIONS and REPORT on the site root also skip it and redirect to well-known CalDAV, so DAVx5 works with the base URL.
              '';
              replaces = ["Google Calendar" "iCloud Calendar" "Google Contacts"];
              privacy = ["calendar" "contacts" "tasks"];
              projectUrl = "https://lennart-k.github.io/rustical/";
              githubUrl = "https://github.com/lennart-k/rustical";
              releaseUrl = "https://github.com/lennart-k/rustical/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "RustiCal CalDAV/CardDAV service configuration";
      };
    };
}
