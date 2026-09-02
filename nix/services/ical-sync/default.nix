# iCal → CalDAV sync via nixpkgs services.vdirsyncer.
{...}: {
  flake.modules.nixos.ical-sync = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services."ical-sync";
      rusticalEnabled = config.neo.services.rustical.enabled or false;
      swagEnabled = config.neo.services.swag.enabled or false;
      hasSubscriptions = cfg.subscriptions != [];

      nonempty = v: v != null && v != "";
      calendarNames = map (s: s.calendar) cfg.subscriptions;
      validCalendarId = s: builtins.match "[A-Za-z0-9_-]+" s != null;

      normalizeIcalUrl = url:
        if hasPrefix "webcal://" url
        then "https://${removePrefix "webcal://" url}"
        else if hasPrefix "webcals://" url
        then "https://${removePrefix "webcals://" url}"
        else url;

      caldavBase = removeSuffix "/" cfg.caldavUrl;
      collectionUrl = sub: "${caldavBase}/principal/${cfg.user}/${sub.calendar}/";

      escapeXml = s: let
        amp = "&" + "amp;";
        lt = "&" + "lt;";
        gt = "&" + "gt;";
        quot = "&" + "quot;";
        apos = "&" + "apos;";
      in
        replaceStrings ["&" "<" ">" "\"" "'"] [amp lt gt quot apos] s;

      pairs = listToAttrs (imap0 (i: sub: {
          name = "sub${toString i}";
          value = {
            a = "sub${toString i}_http";
            b = "sub${toString i}_caldav";
            collections = null;
            conflict_resolution = "a wins";
          };
        })
        cfg.subscriptions);

      storages = listToAttrs (concatLists (imap0 (
          i: sub: [
            {
              name = "sub${toString i}_http";
              value = {
                type = "http";
                url = normalizeIcalUrl sub.url;
              };
            }
            {
              name = "sub${toString i}_caldav";
              value = {
                type = "caldav";
                url = collectionUrl sub;
                username = cfg.user;
                password = cfg.password;
                item_types = ["VEVENT"];
              };
            }
          ]
        )
        cfg.subscriptions));

      mkcalendarBody = sub: ''
        <?xml version="1.0" encoding="utf-8" ?>
        <C:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
          <D:set>
            <D:prop>
              <D:displayname>${escapeXml sub.calendar}</D:displayname>
            </D:prop>
          </D:set>
        </C:mkcalendar>
      '';

      ensureCalendars = pkgs.writeShellScript "ical-sync-mkcalendar" ''
        set -euo pipefail
        create_calendar() {
          local url="$1"
          local body="$2"
          local name="$3"
          local out code
          out=$(mktemp)
          echo "Ensuring calendar $name exists"
          code=$(${pkgs.curl}/bin/curl -sS -o "$out" -w '%{http_code}' \
            -X MKCALENDAR \
            -u ${escapeShellArg "${cfg.user}:${cfg.password}"} \
            -H 'Content-Type: application/xml; charset=utf-8' \
            --data "$body" \
            "$url" || true)
          case "$code" in
            200|201|204|207)
              echo "  created ($code)"
              ;;
            405|409|412|423)
              echo "  already present ($code)"
              ;;
            *)
              echo "  MKCALENDAR failed HTTP $code" >&2
              cat "$out" >&2 || true
              rm -f "$out"
              exit 1
              ;;
          esac
          rm -f "$out"
        }
        ${concatMapStringsSep "\n" (sub: ''
            create_calendar ${escapeShellArg (collectionUrl sub)} ${escapeShellArg (mkcalendarBody sub)} ${escapeShellArg sub.calendar}
          '')
          cfg.subscriptions}
      '';
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = !hasSubscriptions || nonempty cfg.caldavUrl;
            message = "neo.services.ical-sync: caldavUrl must be set when subscriptions are configured (enable RustiCal or set a CalDAV base URL).";
          }
          {
            assertion = !hasSubscriptions || nonempty cfg.user;
            message = "neo.services.ical-sync: user must be set when subscriptions are configured (RustiCal principal id).";
          }
          {
            assertion = !hasSubscriptions || nonempty cfg.password;
            message = "neo.services.ical-sync: password must be set when subscriptions are configured (RustiCal app token from the web UI).";
          }
          {
            assertion = all (s: nonempty s.calendar && nonempty s.url) cfg.subscriptions;
            message = "neo.services.ical-sync: each subscription needs a calendar name and an iCal URL.";
          }
          {
            assertion = all (s: validCalendarId s.calendar) cfg.subscriptions;
            message = "neo.services.ical-sync: calendar ids may only contain letters, digits, hyphen, and underscore.";
          }
          {
            assertion = length calendarNames == length (unique calendarNames);
            message = "neo.services.ical-sync: subscription calendar names must be unique.";
          }
        ];

        services.vdirsyncer = mkIf hasSubscriptions {
          enable = true;
          jobs.ical-sync = {
            forceDiscover = true;
            timerConfig = {
              OnBootSec = "5min";
              OnCalendar = cfg.schedule;
              Persistent = true;
            };
            config = {
              inherit pairs storages;
            };
          };
        };

        systemd.services."vdirsyncer@ical-sync" = mkIf hasSubscriptions {
          after =
            optional rusticalEnabled "docker-rustical.service"
            ++ optional swagEnabled "docker-swag.service";
          serviceConfig.ExecStartPre = [ensureCalendars];
        };
      };
    };
}
