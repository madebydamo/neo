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
      # The vdirsyncer timer is installed only when a subscription exists
      # (see services.vdirsyncer in default.nix). An empty list keeps the
      # service target from synthesizing a stub unit.
      icalUnits =
        if (config.neo.services.ical-sync.subscriptions or []) == []
        then []
        else ["vdirsyncer@ical-sync"];
      defaultCaldavUrl =
        if domain != null && domain != ""
        then "https://${rusticalSub}.${domain}/caldav"
        else "";
    in {
      options.neo.services.ical-sync = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Copy online calendars you subscribe to (school, sports, holidays) into your own calendar" {rank = 0;};

              # CalDAV base URL; default is this host's RustiCal
              # (https://rustical.<domain>/caldav).
              caldavUrl = mkOption {
                type = types.str;
                default = defaultCaldavUrl;
                description = "Address of the calendar server the calendars are copied into. The default is your RustiCal.";
                rank = 10;
                ui = lib.neo.ui.mkUi {
                  group = lib.neo.ui.groups.advanced;
                  label = "Calendar server address";
                };
              };

              user = mkOption {
                type = types.str;
                default = "";
                # CalDAV username = RustiCal principal id.
                description = "Your calendar username (usually your login name).";
                rank = 20;
              };

              password = lib.neo.mkSecretOption {
                type = types.str;
                default = "";
                # CalDAV password; for RustiCal an app token (not ssoPassword).
                description = "An app password created in RustiCal (Frontend › App tokens), not your normal login password.";
                rank = 30;
              };

              subscriptions = mkOption {
                type = types.listOf (types.submodule {
                  options = {
                    calendar = mkOption {
                      type = types.str;
                      # RustiCal collection name on the CalDAV server.
                      description = ''
                        Name of the calendar to copy into, for example school-holidays. Letters, digits, - and _ only.
                        It is created if it does not exist yet.
                      '';
                      rank = 0;
                    };
                    url = mkOption {
                      type = types.str;
                      description = "Link to the online calendar, usually ending in .ics or starting with webcal://.";
                      ui = lib.neo.ui.mkUi {
                        label = "Calendar link";
                      };
                      rank = 10;
                    };
                  };
                });
                default = [];
                description = ''
                  Online calendars to copy into your own calendars.
                  The online calendar wins: new events are added, and events removed there are removed from your copy too.
                '';
                rank = 40;
              };

              schedule = mkOption {
                type = types.str;
                default = "*-*-* 00/6:00:00";
                # systemd OnCalendar expression.
                description = ''
                  How often calendars refresh (default every 6 hours).
                  For example "hourly", "daily" or "*-*-* 00/6:00:00" for every 6 hours.
                '';
                ui = lib.neo.ui.mkUi {
                  label = "Refresh schedule";
                };
                rank = 50;
              };
            }
            // lib.neo.mkSystemdUnits icalUnits
            // lib.neo.mkSystemdTimers icalUnits
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              license = {
                spdx = "BSD-3-Clause";
                url = "https://github.com/pimutils/vdirsyncer/blob/main/LICENSE";
              };
              iframeCompatible = false;
              icon = ./icon.svg;
              tagline = "Adds public calendars like school holidays to your calendar";
              description = ''
                Copies public calendar feeds, such as school holidays, waste collection, sports fixtures or public holidays, into your own calendar so they show up on every phone.
                The feeds are refreshed regularly.
              '';
              notes = ''
                Uses vdirsyncer to mirror .ics/webcal feeds into a CalDAV calendar (RustiCal on this host by default). Missing calendars are created.
                Log in with your RustiCal user and an app token from the RustiCal UI, not the tinyauth password and not ssoPassword.
                The feed wins: events removed upstream are deleted in your calendar too.
              '';
              privacy = ["calendar"];
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
