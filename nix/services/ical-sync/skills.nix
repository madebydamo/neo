# Hermes skill for ical-sync.
{...}: {
  flake.modules.nixos.ical-sync-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services."ical-sync";
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services."ical-sync".skill.conf = lib.neo.mkServiceSkill {
      service = "ical-sync";
      inherit cfg domain;
      description = "iCal feed sync into CalDAV via vdirsyncer";
      tags = ["neo" "ical-sync" "vdirsyncer" "caldav" "calendar"];
      body = ''
        ## When to Use
        Subscribing to public iCal/webcal feeds, mirroring them into RustiCal (or another CalDAV server), missing events, failed sync runs.

        ## Architecture notes
        - NixOS `services.vdirsyncer` job `ical-sync` → `vdirsyncer@ical-sync.service` / `.timer`
        - Each subscription is HTTP `.ics` (read-only) paired with one CalDAV collection
        - Default CalDAV base: `https://rustical.<domain>/caldav` (DAV is on RustiCal publicPaths, app token HTTP Basic)
        - Collection URL: `<caldavUrl>/principal/<user>/<calendar>/`
        - Missing collections are created with MKCALENDAR (`ExecStartPre`) then `forceDiscover` + sync
        - iCal side wins (`conflict_resolution = a wins`); events deleted upstream are removed on CalDAV (`partial_sync` default revert)
        - Status data: `/var/lib/vdirsyncer/ical-sync`

        ## Credentials
        - `services.ical-sync.user`: RustiCal principal id (usually the tinyauth username)
        - `services.ical-sync.password`: **RustiCal app token from the RustiCal web UI** (Frontend → app tokens). Not `ssoPassword`, not tinyauth
        - `services.ical-sync.caldavUrl`: CalDAV base URL (override only for a non-RustiCal server)

        ## Procedures
        1. Enable RustiCal, open the UI, generate an app token, paste it into `ical-sync.password`
        2. Set `user` to that principal id
        3. Add subscriptions: `calendar` (collection id) + `url` (`https://…ics` or `webcal://…`)
        4. `systemctl start vdirsyncer@ical-sync` then `journalctl -u vdirsyncer@ical-sync -b --no-pager`
        5. Confirm the calendar in RustiCal / Calino / a CalDAV client

        ## Pitfalls
        - Calendar ids: `[A-Za-z0-9_-]+` only; names must be unique
        - Principal must already exist (SSO provision or a prior RustiCal login)
        - Hairpin/DNS: sync uses the public CalDAV URL; local DNS (Pi-hole) or a reachable domain is required
        - Feeds that rotate UIDs every fetch cause churn; vdirsyncer hashes HTTP items to reduce that
        - Removing a subscription from settings does not delete the CalDAV calendar

        ## Verification
        - Timer listed: `systemctl list-timers vdirsyncer@ical-sync`
        - Last run in the journal shows discover + sync without errors
        - Events from the .ics appear on the named calendar; deleting one from the feed removes it on the next sync
      '';
    };
  };
}
