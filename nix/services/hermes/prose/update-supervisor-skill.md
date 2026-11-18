## When to Use
Unattended run after `neo-auto-update` or `neo-docker-updater`. Preloaded by `neo-hermes-supervise`. Also use when the operator asks why an update was rolled back or notified.

## Outcomes (exactly one)
1. **clean** — updater succeeded, units healthy, logs have no migration/deprecation/warning that the operator should see. **Do not send a message.**
2. **warning** — units are running but logs show migration hints, deprecations, non-fatal warnings, or something notable that is not an outage. `hermes send --to all` with a short summary. **Do not roll back.**
3. **broken** — unit failed/inactive, crash loop, or the app is clearly unusable from logs. `hermes send --to all`. For **docker** only, roll back the broken image. For **system**, never roll back a NixOS generation.

## Inputs
- Latest marker (symlink, retargeted when a run *starts*): `@appdata@/updater/system/last.json` or `@appdata@/updater/docker/last.json`. `in_progress=true` means that run crashed — treat as failed, do not use a previous run.
- History: `@appdata@/updater/system/` and `@appdata@/updater/docker/` (each service's appdata) — one `<utc>-<pid>.json` plus matching `.log` per run (never overwritten)
- Journals: `journalctl -u <updater-unit> -b --no-pager` plus each consumer unit; prefer the run `.log` next to the JSON
- `systemctl status` / `systemctl --failed`
- Docker: `docker logs <container> --tail 200` for each updated unit in the marker

Skip Hermes entirely when the marker says `changed=false` and `failed=false` (the wrapper already does this). If you were launched, something changed or failed — still apply the three-way classification.

## Docker rollback
The marker lists each changed image (`image`, `old_id`, `new_id`, `units`). Replaced versions stay on disk (tagged `neo-history/<image-with-/-:-as-__>:<12-hex-id>`) for the updater's "Delete old versions after" time; older ones are downloaded again by digest.

Roll back **only** images you judged **broken** (not warnings):

```bash
sudo neo-docker-rollback --previous --image '<repo:tag>'
# or every image the last run changed (only if every update is broken):
sudo neo-docker-rollback --previous
```

It retags the image, restarts its units and checks they are active. Settings are not changed: the next scheduled update tries the new version again, so say in the notification what broke. If rollback fails, say so in the notification.

Do not invent `docker tag` commands. Do not roll back images that are merely warning.

## Notifications
```bash
hermes send --to all "…"
```
Home channels come from `/sethome`. If that fails, `hermes send --to telegram`
(Neo sets `TELEGRAM_HOME_CHANNEL` to the first `telegramAllowedUserId`).
No message on **clean**. Do not paste secrets, tokens, or full `.env` files.

## System updates
Read `neo-auto-update` logs and `systemctl --failed`. Switch failure or units that did not come up → **broken** notify, no `nixos-rebuild --rollback`. Operator wants the diagnosis, not an automatic generation bounce.

## After a restart
When a system run needs a restart, it is supervised in the boot after it (`neo-hermes-supervise restart`; when the updater scheduled the restart, the run is not supervised before it). The run's JSON has `restart_check`: `booted_expected=false` (not on the installed system) or `system`/`kernel` in `reasons_left` → **broken**; other `reasons_left` → **warning**; `failed_units` → look at `journalctl -b -u <unit>`. The update itself ran in the previous boot (`journalctl -b -1`).

## Pitfalls
- `neo-docker-updater` used to ignore restart failures; trust unit state and container logs, not the updater exit code alone
- First-time pulls have `old_id=none` — cannot roll back; notify only
- Shared images restart every consumer; one bad app must not retag an image still needed by a healthy sibling unless that image is the cause
