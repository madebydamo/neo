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
5. **Cache.** `verdicts.json`, keyed by origin|vuln|package|version. An
   operator verdict (`neo security accept`) always wins; AI verdicts are reused
   until `neo security triage --all` or `reset`.
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

## AI source

`ai.source = auto` uses Hermes when its gateway token turns on the API server
(`127.0.0.1:gatewayPort/v1`, Bearer = gateway token), else no review.
`custom` takes `baseUrl` (+ `apiKey`, `model`). The key reaches the scan as a
root-only secrets template and goes to curl on stdin, never argv. Package
names and CVE details leave the machine to that service.

## Not done

- OpenVEX export of the verdicts for other tools.
- Accept / reset from the page (the page is read-only; use the CLI).
