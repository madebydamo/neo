#!/usr/bin/env bash
#
# SWAG certificate reloader.
# Periodically checks mtime of fullchain.pem symlinks under the live/ directory
# (handles per-domain subdirs created by certbot/SWAG) and reloads nginx inside
# the SWAG container when updates are detected. Robust to live/ and subdomain
# dir creation/deletion during initial issuance or re-issuance.

set -uo pipefail

: "${WATCH_DIR:=/var/neo/DATA/AppData/swag/etc/letsencrypt/live}"
: "${CONTAINER_NAME:=swag}"
: "${NGINX_RELOAD_CMD:=nginx -c /config/nginx/nginx.conf -s reload}"

log() { echo "$(date '+%F %T') [swag-cert] $*"; }

get_latest_mtime() {
  local m
  m=$(find "$WATCH_DIR" -name fullchain.pem -exec stat -c %Y {} + 2>/dev/null | sort -rn | head -1)
  echo "${m:-0}"
}

reload_nginx() {
  log "Triggering nginx reload in container '$CONTAINER_NAME'..."
  if docker exec "$CONTAINER_NAME" $NGINX_RELOAD_CMD; then
    log "nginx reload successful"
  else
    log "ERROR: nginx reload failed (container may not be running or nginx config error)"
  fi
}

wait_for_watch_dir() {
  if [ -d "$WATCH_DIR" ]; then
    log "Watch directory ready: $WATCH_DIR"
    return
  fi
  log "Directory does not exist yet: $WATCH_DIR — waiting..."
  while [ ! -d "$WATCH_DIR" ]; do
    sleep 5
  done
  sleep 2
  log "Watch directory ready: $WATCH_DIR"
}

main() {
  log "Starting SWAG certificate reloader (watching $WATCH_DIR)"
  last_mtime=0
  while true; do
    wait_for_watch_dir

    current=$(get_latest_mtime)
    if [ "$current" -gt "$last_mtime" ]; then
      if [ "$last_mtime" -ne 0 ]; then
        log "Detected updated certificate (mtime $current > $last_mtime) — reloading"
        reload_nginx
      fi
      last_mtime=$current
    elif [ "$last_mtime" -eq 0 ] && [ "$current" -gt 0 ]; then
      last_mtime=$current
    fi
    log "Monitoring certificate mtimes (baseline $last_mtime)..."

    while [ -d "$WATCH_DIR" ]; do
      sleep 30
      current=$(get_latest_mtime)
      if [ "$current" -gt "$last_mtime" ]; then
        log "Detected updated certificate (mtime $current > $last_mtime)"
        reload_nginx
        last_mtime=$current
      fi
    done

    log "Watch directory disappeared — re-watching"
    sleep 2
  done
}

main "$@"
