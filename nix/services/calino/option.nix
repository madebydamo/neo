# Calino (browser CalDAV client) service options.
# Web UI protected with tinyauth. CalDAV traffic is browser-to-server; when
# RustiCal is also enabled, SWAG adds CORS on RustiCal DAV paths for this origin.
# ICS/webcal subscribe is a browser GET; SWAG /webcal-proxy/ fetches the feed
# so publishers that omit CORS still work. iframeCompatible stays default true:
# SWAG hides upstream X-Frame-Options via neo.iframeCookieSupport so the
# navigator can embed the UI.
{...}: {
  flake.modules.nixos.calino-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.calino = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Calendar, tasks and contacts in your browser, using the calendars on your own server" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8080;
                description = "Internal port Calino listens on";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "calino";
              auth.enabled = true;
            }
            // lib.neo.mkContainerDefinitions {
              calino = "ghcr.io/ivan-malinovski/calino:latest";
            }
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              license = {
                spdx = "MIT";
                url = "https://github.com/Ivan-Malinovski/calino/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Your calendars and contacts in the browser";
              description = ''
                A calendar app in your browser with month, week and day views, to-dos and contacts.
                It shows the calendars you keep in RustiCal (or another calendar server), and can also show public calendars such as school holidays.
              '';
              notes = ''
                Calino is a static web app; calendars stay on your CalDAV server and there is no Calino account or database.
                When RustiCal is enabled, SWAG adds CORS headers on RustiCal's calendar paths so the browser can talk to it directly.
                Public .ics feeds almost never send CORS headers, so SWAG proxies them at https://calino.<domain>/webcal-proxy (behind tinyauth). In Calino choose Subscribe to calendar, set Proxy URL to that address and paste the original .ics URL.
                Calendar passwords (RustiCal app tokens) are entered in Calino and stored in the browser.
              '';
              replaces = ["Google Calendar" "Outlook Calendar"];
              privacy = ["calendar" "contacts"];
              projectUrl = "https://calino.io";
              githubUrl = "https://github.com/Ivan-Malinovski/calino";
              releaseUrl = "https://github.com/Ivan-Malinovski/calino/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Calino CalDAV client service configuration";
      };
    };
}
