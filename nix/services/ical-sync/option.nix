# iCal → CalDAV subscription sync (vdirsyncer). No public UI.
{...}: {
  flake.modules.nixos.ical-sync-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      domain = config.neo.services.swag.domain or null;
      rusticalSub = config.neo.services.rustical.subdomain or "rustical";
      defaultCaldavUrl =
        if domain != null && domain != ""
        then "https://${rusticalSub}.${domain}/caldav"
        else "";
    in {
      options.neo.services.ical-sync = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "iCal subscription sync into a CalDAV server (vdirsyncer)" {rank = 0;};

              caldavUrl = mkOption {
                type = types.str;
                default = defaultCaldavUrl;
                description = ''
                  CalDAV base URL to write subscribed calendars into.
                  Defaults to this host's RustiCal (`https://rustical.<domain>/caldav`).
                '';
                rank = 10;
              };

              user = mkOption {
                type = types.str;
                default = "";
                description = ''
                  CalDAV username (RustiCal principal id, usually the same as the tinyauth username).
                '';
                rank = 20;
              };

              password = mkOption {
                type = types.str;
                default = "";
                description = ''
                  CalDAV password. For RustiCal this is an app token from the RustiCal web UI (Frontend → app tokens), not ssoPassword and not the tinyauth password.
                '';
                rank = 30;
              };

              subscriptions = mkOption {
                type = types.listOf (types.submodule {
                  options = {
                    calendar = mkOption {
                      type = types.str;
                      description = ''
                        Destination calendar id on the CalDAV server (RustiCal collection name). Letters, digits, hyphen, and underscore only. Created automatically if missing.
                      '';
                      rank = 0;
                    };
                    url = mkOption {
                      type = types.str;
                      description = "Remote iCal URL to subscribe to (https://…/calendar.ics or webcal://…)";
                      rank = 10;
                    };
                  };
                });
                default = [];
                description = ''
                  iCal feeds to mirror into CalDAV. Each entry names a calendar on the server and the remote .ics URL.
                  The feed is the source of truth: new events are added, and events that disappear from the feed are removed from the calendar.
                '';
                rank = 40;
              };

              schedule = mkOption {
                type = types.str;
                default = "*-*-* 00/6:00:00";
                description = "systemd OnCalendar schedule for subscription sync (default every 6 hours)";
                rank = 50;
              };
            }
            // lib.neo.mkSystemdUnits ["vdirsyncer@ical-sync"]
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              iframeCompatible = false;
              icon = "https://api.iconify.design/mdi/calendar-sync-outline.svg";
              description = ''
                ical-sync subscribes to public iCal feeds and mirrors them into a CalDAV calendar with vdirsyncer.
                Point it at a CalDAV server (RustiCal on this host by default), a principal, and an app token copied from the RustiCal UI — not the tinyauth password and not ssoPassword.
                Each subscription is a calendar name plus an https or webcal .ics URL. Missing calendars are created. The iCal feed wins: events removed upstream are deleted on the CalDAV side too.
              '';
              projectUrl = "https://vdirsyncer.pimutils.org/";
              githubUrl = "https://github.com/pimutils/vdirsyncer";
              releaseUrl = "https://github.com/pimutils/vdirsyncer/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "iCal to CalDAV subscription sync";
      };
    };
}
