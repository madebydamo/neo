# Calino (browser CalDAV client) service options.
# Web UI protected with tinyauth. CalDAV traffic is browser-to-server; when
# RustiCal is also enabled, SWAG adds CORS on RustiCal DAV paths for this origin.
# iframeCompatible stays default true: SWAG hides upstream X-Frame-Options via
# neo.iframeCookieSupport so the navigator can embed the UI.
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
              enabled = mkEnableOption "Calino browser CalDAV calendar client" {rank = 0;};
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
              icon = "https://raw.githubusercontent.com/Ivan-Malinovski/calino/main/public/calino-icon.svg";
              description = ''
                Calino is a lightweight browser CalDAV client: month/week/day views, tasks, journals, and CardDAV contacts, with no Calino-side account or database.
                The Docker image is a static SPA; calendars stay on your CalDAV server (RustiCal on this host, or any RFC 4791 server you configure in the UI).
                Neo protects the UI with tinyauth. When RustiCal is enabled, SWAG adds CORS on RustiCal DAV paths for the Calino origin so the browser can talk to CalDAV without a proxy.
                CalDAV credentials (RustiCal app tokens) are entered in Calino and stored in the browser. Upstream X-Frame-Options is stripped at SWAG so the neo navigator can embed the UI.
              '';
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
