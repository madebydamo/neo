# Security scan

`services.security` (off by default) checks the
NixOS system and every container image for known vulnerabilities, has an AI
sort out the false alarms and shows the result on its own subdomain behind
tinyauth. It is not part of the neo web UI: most findings are upstream work
(nixpkgs, image maintainers) that a user cannot act on.

## Pieces

| Piece | Where |
|-------|-------|
| Options, units, SWAG | `nix/services/security/` |
| CLI (`neo security scan/triage/show/accept/reset/serve`) | `cli/src/commands/security/` |
| Report page | `cli/templates/security/report.html.hbs`, neo's `neo-ui.css` |
| Data | `<appdata>/security/report.json`, `verdicts.json`, `work/` |
| Scanner caches (NVD, grype, Trivy, OSV) | `/var/cache/neo-security` (not snapshotted) |

`neo-security-scan.service` (root, timer `schedule`) and
`neo-security-web.service` (homeserver, loopback `port`, read-only appdata).
Nix writes the settings (also at `/etc/neo/security.json`, so the CLI works
from a root shell) plus a map of every enabled service (units,
containers → image, URL, ingress, tinyauth) to a JSON file
(`NEO_SECURITY_CONFIG`).

## Pipeline

1. **Scan.** `vulnxscan --triage` (vulnix + grype + OSV over the runtime
   closure of `/run/current-system`; CVEs named in a derivation's `patches`
   are dropped; repology adds a fix class). Repology often fails, then the
   scan repeats without `--triage`. Trivy over each image a container uses
   (`--image-src docker`, by image id, no pull).
2. **Group.** Findings below `minSeverity` are dropped (unknown severity counts
   as medium). Hits with the same vulnerability × package × version merge into
   one group, listing every image, container and service.
3. **Evidence.** `/proc/*/maps` and `exe` of every process, attributed by
   cgroup to a systemd unit or a container. Nix: is a store path
   `<pname>-<version>` mapped. Images: is a file of the OS package (dpkg list or
   apk db read through `/proc/<pid>/root`) mapped by the container's processes.
   Plus exposure (URL, ingress, tinyauth) per service and notes (only vulnix =
   name match; repology class; Trivy status).
4. **Rules.** Repology says not vulnerable → harmless. Trivy `will_not_fix` →
   waiting. No AI needed.
5. **Cache and decisions.** AI verdicts are cached in `verdicts.json` (the
   scan's), keyed by origin|vuln|package|version, and reused until
   `neo security triage --all`. The operator's decisions live in
   `decisions.json`, written by the page's buttons (as homeserver) and
   `neo security accept|reset`, only read by the scan, and laid over every read
   of the report, so a decision taken during a running check is not lost:
   - **Not relevant here** (accept) wins over any verdict, and carries over to
     later versions of the package while the finding is still reported (with
     the note "accepted for version X; still reported in Y").
   - **Review again** / **Undo** hides older verdicts and queues a review-only
     run (request `triage`) in which those findings go first.
   An AI verdict for an earlier version goes to the review of the new version as
   `previous_verdict`.
   Findings whose vulnerability × package (any version) was not in the previous
   report are marked **new** (badge and filter).
6. **AI review** of the open groups, most severe / internet-facing / loaded
   first, up to `ai.maxReviews` requests per run. One OpenAI-compatible chat
   completion per batch (up to 8 findings of the same package, version and
   places, which share their evidence) with the evidence and each OSV record in
   the prompt; the answer is a JSON verdict per vulnerability (`attention` / `waiting` / `harmless`, confidence, summary,
   explanation, action). Nothing depends on tool calls, so it works with
   Hermes' API server (which runs its own agent and may inspect the system
   read-only) and plain model endpoints alike. Guard: a critical finding in an
   internet-facing service is only dismissed with high confidence.

The report is saved after every review, so the page shows progress.

## Triggers

`neo-security-scan.service` runs from the timer (`schedule`), the page's
**Check now** and, with `scanAfterUpdates`, after updates. Requests are a file,
`/run/neo-security/scan-request` (`manual`, `triage`, `update`,
`update <system>`), watched by `neo-security-scan.path`, so the page needs no
privileges. An activation script requests `update <new system>` on a switch to
another system (not at boot); neo-docker-updater's `ExecStartPost` requests
`update`. An `update` request is skipped when the running system and the set of
image ids equal the last report's. `scanAfterUpdates` is off by default (each
check costs AI requests), so only the timer and Check now start one.

A switch never touches the scan unit (`restartIfChanged = false`,
`stopIfChanged = false`): restarting a changed oneshot made `neo activate`
wait for the whole scan and its AI review. A running check goes on with the
old config; the next one uses the new.

## AI source

`ai.source = auto` uses Hermes when its gateway token turns on the API server
(`127.0.0.1:gatewayPort/v1`, Bearer = gateway token), else no review.
`custom` takes `baseUrl` (+ `apiKey`, `model`). The key reaches the scan as a
root-only secrets template and goes to curl on stdin, never argv. Package
names and CVE details leave the machine to that service.

## Chat

With Hermes' API on, the report page carries neo web's chat drawer: the same
routes (`web/chat.rs`), drawer, `@` popup and Hermes session client, with the
page's own `ChatHost` (`cli/src/commands/security/chat.rs`). The header
Assistant button opens `security:general` (the report summary goes with the
first message); "Ask the assistant" on a finding opens `security:<id>` with that
finding's evidence and verdict. `@` lists the findings, tokens are the CVE id
(`@CVE-2026-1234`, or `@CVE-…:<package>` when several packages share it); a
tagged finding goes along as a `<neo-references>` block. These chats are
listed under Chats on the report page only, neo web's only there. The page key
is the gateway token (`neo-security-chat-key`, readable by homeserver).

## Not done

- OpenVEX export of the verdicts for other tools.
- Accept / reset from the page (the page is read-only; use the CLI).
