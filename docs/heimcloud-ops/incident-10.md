# Heimcloud Ops incident #10

Phase 1 draft PR shell — fill in the repair. **Do not auto-merge.**

## Incident

- **report_hash:** `c51ec63a3d9bfc36e48e49fa49883cc01b98d5bd46cd05d9899914ed9530f771`
- **severity:** warning
- **class:** unknown
- **status:** open
- **neo_version:** neo 0.1.0 / nixos-system-hattori-26.05.20260922.1bc55b9 / gen system-330-link
- **unit:** neo-docker-updater / docker-searxng
- **customer_repo_slug:** KAKJWG9RM5
- **target_hint:** hattori searxng: stale engines settings vs image after neo-docker-updater 20260924T142526Z — prune removed engines from settings or pin older image; rollback available via neo-docker-rollback --image 'docker.io/searxng/searxng:latest'

## Plugin URLs

```
(none)
```

## Logs excerpt

```
Docker update run 20260924T142526Z-2820211 (failed=false changed=true).
Image docker.io/searxng/searxng:latest
  old: sha256:0986ff79a38e4d59255cb62cfc7e27b1d78aae751d6e42e27edf9c5ddb4e1ad9
  new: sha256:d21f6bcbf34f555474589ff91f0994c9d63750a093184276463a8d29a644c133
  rollback_tag: neo-rollback:docker.io__searxng__searxng__latest
  units: docker-searxng
systemctl: docker-searxng=active, docker-searxng-redis=active
healthz via swag→searxng:8080 = HTTP 200
After restart, searx.engines ERROR: can't register engine (loading engine failed) FileNotFoundError for engine modules still referenced by settings. Missing modules seen: adobe_stock.py,aol.py,cara.py,loc.py,podcastindex.py,presearch.py,reddit.py,svgrepo.py
Also: missing /etc/searxng/limiter.toml; X-Forwarded-For nor X-Real-IP header is set (botdetection).
Classification: warning (operator-requested ops incident). Core search UI up; subset of engines dead until settings.yml pruned or modules restored. No auto-rollback.
```

## Checklist for Repair / human

- [ ] Confirm class (software vs human_config)
- [ ] Reproduce or verify from logs
- [ ] Implement fix on this branch
- [ ] Request review — **no auto-merge**

---
_Opened by Heimcloud Ops · branch `heimcloud/incident-10`_
