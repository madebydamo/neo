# Docker image retention and rollback (proposal)

Status: implemented.

## Problem

`/var/lib/docker` lives on the root dataset (`zroot/root`, never snapshotted) and only grows:

- Every `neo-docker-updater` run that moves a tag (`immich:release`, `pihole:latest`, …) leaves the previous image behind, untagged (dangling) or held by a `neo-rollback:*` tag (Hermes `superviseUpdates`). Nothing deletes them.
- Changing a container's image in settings, disabling a service or removing a plugin leaves its images on disk.

We still want to go back to the image that ran at a given time (after a restore, after a bad update), without keeping every image of every snapshot.

## Design

Same model as the system updater: **old versions are kept for a configurable time, garbage collection runs at the start of each scheduled update run.** The docker updater is also the one that records which image ran when.

### 1. Option

`neo.services.docker-updater.garbageCollectOlderThan` (like `system-updater.garbageCollectOlderThan`):

- type `nullOr str`, default `"30d"`, `null` = never delete.
- Description (draft): "Delete old app versions after this long (e.g. 30d) to free disk space. Older versions can no longer be rolled back to without downloading them again."

No UI button, no other settings.

### 2. Image ledger (written by the updater)

The updater keeps, per image reference, which image ran during which time span:

```json
{
  "updated_at": "2026-11-18T04:12:00Z",
  "images": {
    "ghcr.io/immich-app/immich-server:release": {
      "units": ["docker-immich-server"],
      "versions": [
        { "id": "sha256:4f1c…", "digest": "ghcr.io/immich-app/immich-server@sha256:9ab2…", "from": "2026-11-16T04:03:11Z", "until": null, "local": true },
        { "id": "sha256:c3d9…", "digest": "ghcr.io/immich-app/immich-server@sha256:77e0…", "from": "2026-11-09T04:01:40Z", "until": "2026-11-16T04:03:11Z", "local": true }
      ]
    }
  }
}
```

- `id` = local image id (what we retag); `digest` = registry digest from `RepoDigests` (what we can pull again once the local copy is gone). `null` digest = local or Nix-built image, not re-pullable.
- `until = null` = current version. Times are UTC; `local` = the image is still on disk.
- Authoritative copy: `/var/lib/neo/docker-images.json` (root dataset). It must not travel back with a data restore, otherwise GC would forget images pulled after the snapshot.
- Each run also writes a copy into its run history (`${appdata}/updater/docker/images.json`, on the snapshotted dataset), so every ZFS snapshot records which versions were running at that time.

Each run updates it:

- After a pull that changed the image: close the old version (`until = now`), add the new one.
- An image reference that is no longer in the plan (tag changed in settings, service disabled, plugin removed): close its current version with `until = now`. The updater only notices this at its next run, so such images are kept up to one schedule interval longer than the setting says; that errs on the safe side.
- A reference seen for the first time: add it with `from = now`.

### 3. Garbage collection (first step of every run)

Before pulling anything, under the system lock the run already holds. The rule is a keep set; **every other image is deleted**, whoever built or pulled it, tagged or not.

Keep set:

- **Running:** the images of every enabled container (the updater plan, current ledger versions) and every image an existing container uses (running or stopped; Docker refuses to delete those anyway, never `-f`).
- **Holdback:** closed ledger versions whose `until` is younger than `garbageCollectOlderThan`.

Steps:

1. List all images (`docker image ls -a --no-trunc`), remove every image id not in the keep set (`docker rmi` by id, which also drops all its tags). That covers expired versions, dangling layers, old `neo-rollback:*` tags, your own and agents' builds that no container uses, and Nix-built plugin images that are not running (the plugin loads them again from the Nix store when it starts).
2. Removed ledger versions keep their digest with `"local": false`, so a rollback can still pull them. Entries without a local image and with `until` older than, say, a year are dropped, so the file stays small.
3. `docker builder prune -af` (build cache is not an image anyone runs).

Not touched: volumes (data, not images).

Consequence: an image built or pulled by hand and not yet started is deleted at the next run. Start it (or keep its container around) before then, or build it again.

Holdback versions stay tagged `neo-history/<ref-with-/-and-:-as-__>:<12-hex-id>` so `docker images` shows what is kept and why. This replaces `neo-rollback:*`.

Migration: none needed. The first run deletes everything outside the keep set, including today's pile of old versions (their digests were never recorded, so they could not be rolled back to by time anyway).

### 4. Rollback = retag, no settings change

`neo-docker-rollback` gets:

- `--at <time>`: for each image (or `--image REF`), take the version whose `from`/`until` span covers that time.
- `--previous` (today's behaviour, Hermes): the newest closed version.

For each selected version: if the image is still local, `docker tag <id> <ref>`; otherwise `docker pull <digest>`, then tag. Then restart the image's units (with the unit locks, like the updater). `settings.toml` is never changed.

So the app runs the old version until the next scheduled update, which pulls the tag again and moves forward as usual. Users who want to stay on a version set the image tag/digest in the container's settings themselves; Neo never pins for them, so auto updates keep working.

If the digest has been deleted from the registry and the image is no longer local, that image is skipped and reported.

### 5. Where rollback is offered

- Hermes `superviseUpdates`: `neo-docker-rollback --previous` (as now, new tag name).
- Full data restore (Versioning tab): after the reboot, "Return apps to the versions from <snapshot time>" runs `--at <snapshot time>`.
- Per-service appdata restore: checkbox "Also return the app to the version it ran then", `--image` for that service's images, `--at <snapshot time>`. Useful because a newer app on older data is the usual reason a restore breaks.

The time a snapshot was taken is enough: the ledger on the root dataset knows what ran then (the copy inside the snapshot is a fallback for versions whose entries were already dropped).

## Implementation

| Where | What |
|-------|------|
| `cli/src/utils/docker_ledger.rs` | Ledger model and pure logic with unit tests: `reconcile`, `keep_set`, `select`, `history_tag`, times and ages |
| `cli/src/commands/docker_ledger.rs` | `neo docker-ledger reconcile / gc / select` (hidden subcommand, root): the docker calls. An exclusive `flock` on `/var/lib/neo/docker-images.lock` serialises updater and rollback |
| `nix/services/docker-updater/update.sh` | Driver: `gc` first (or `reconcile` when the age is null), pulls, `reconcile --copy` after every changed image and at the end |
| `nix/services/docker-updater/rollback.sh` | `neo-docker-rollback`: `select`, retag or pull by digest, `reconcile`, restart with the unit locks |
| `nix/services/docker-updater/default.nix` | env (`NEO_CLI`, `NEO_LEDGER`, `NEO_KEEP_REFS`, `NEO_GC_OLDER_THAN`), rollback always installed, sudo for the web UI |
| `cli/src/commands/web/zfs/{apps,data,service}.rs` | Restore offers |

Details that go beyond the design above:

- `reconcile` is the one ledger update: for each planned reference it compares the local image id with the current version (new id → close and add), closes references not in the plan, sets `local` from the images on disk, and drops versions that are gone and either not pullable or closed more than a year ago. A rollback is recorded the same way (the old id becomes the current version from now on).
- The keep set also holds every image named in `virtualisation.oci-containers` (`NEO_KEEP_REFS`) and the current local id of every planned reference, so nothing a declared container needs is deleted even when the ledger misses it. If listing containers fails, GC deletes nothing.
- GC lists `docker image ls -a`: with the containerd image store, the dangling image a pull leaves behind is only listed with `-a`. (`docker build` over an existing tag deletes the old image itself there, so builds are not a stand-in for pulls in tests.) Removal is without `-f`: each tag is removed, then the id if it is still there. Stale `neo-history/*` tags and the old `neo-rollback:*` tags are removed. Every ledger update during a run and every rollback also moves the `neo-history` tags (`reconcile --older-than`, deletes nothing), so right after a pull the replaced version is tagged and the current one is not.
- `--older-than` takes `30d`, `12h`, `90m` (minutes), `2w`, `45s`.
- `neo-docker-rollback`: `--previous` is the default; without `--image`/`--unit` it covers the images the last run changed (Hermes). `--at` takes RFC 3339 or seconds since the epoch. `--unit` selects the references a unit uses; `--no-restart` only retags. The run history copy (`images.json`) is the fallback ledger, which after a full data restore is the snapshot's copy.
- Per-service restore: next to **Restore**, **Restore + app version** appears when the ledger knows another version of the service's images at the snapshot time. The retag happens while the units are down, so they start on the old version.
- Full data restore: scheduling stores the snapshot time in the pool property `neo:restore-apps-at`; after the reboot the success banner shows **Return apps to the versions from …** when any app ran another version then. Dismiss or Cancel forgets it.

## Testing (VM)

- Set the age to `1m`, pull two versions of a test image through a local registry, run the updater twice: the old version is removed only after the age passes, current stays, ledger keeps its digest.
- `neo-docker-rollback --at <before>` with the image still local (retag only) and after GC (pull by digest); the next updater run moves forward again.
- Change a tag in settings: old image closed at the next run, removed after the age.
- A running `:local` image stays; an unused user build (tagged or not) and a stopped plugin's Nix-built image are removed; a volume is never removed.
