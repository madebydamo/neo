# RustiCal (CalDAV/CardDAV) service options.
# Web UI protected with tinyauth; DAV/discovery/health bypass via publicPaths.
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
              enabled = mkEnableOption "RustiCal CalDAV/CardDAV calendar and contacts server" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 4000;
                description = "Internal port RustiCal listens on";
              };
              ssoPassword = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  Shared secret used to complete RustiCal frontend login after tinyauth.
                  Not a CalDAV password: native clients still use app tokens. Generate once and keep stable.
                  When set (and tinyauth is on), Neo provisions a principal per tinyauth user and skips the RustiCal login form.
                '';
                helper = lib.neo.helpers.randomToken;
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
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/davx5.svg";
              description = ''
                RustiCal is a lightweight CalDAV and CardDAV server written in Rust, storing everything in a single SQLite database that is easy to back up.
                It serves calendars and contacts to DAVx5, Apple Calendar, Thunderbird, Evolution, GNOME, Home Assistant, and similar clients, with WebDAV Push for near-instant sync, a Nextcloud-compatible login flow, and Apple configuration profiles.
                Neo exposes the web frontend behind tinyauth and, when ssoPassword is set, signs you into RustiCal as the tinyauth user so there is no second login form.
                CalDAV/CardDAV, well-known discovery, and the /ping health endpoint bypass edge auth so native clients can use RustiCal app tokens.
              '';
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
