#!/usr/bin/env bash
#
# SWAG patcher.
# One-shot script run after the docker-swag container starts. It waits for
# key config files to appear in the bind-mounted appdata volume and applies
# Neo-specific patches:
#   1. Adds an include for /config/nginx/conf.d/*.conf into nginx.conf
#   2. Optionally injects or removes iframe/embed CSP/frame-ancestors support
#      into proxy.conf (controlled by neo.iframeCookieSupport + neo.enabled)
#   3. Drops the swag-dashboard mod's fixed dashboard.subdomain.conf when Neo
#      serves the UI under a different subdomain (default: swag)

set -uo pipefail

: "${APPDATA:=/var/neo/DATA/AppData/swag}"
: "${NEO_UID:=1000}"
: "${NEO_GID:=1000}"
: "${NEO_SUPPORT:=false}"
: "${DASHBOARD_SUBDOMAIN:=swag}"

# ====================
# 1. nginx.conf patcher (conf.d include)
# ====================
NGINX_CONF="$APPDATA/nginx/nginx.conf"
echo "=== Patching nginx.conf ==="

for i in $(seq 1 60); do
  if [ -f "$NGINX_CONF" ]; then
    break
  fi
  sleep 1
done

if [ -f "$NGINX_CONF" ]; then
  if ! grep -qE '^[[:space:]]*include[[:space:]]+/config/nginx/conf\.d/\*\.conf;' "$NGINX_CONF"; then
    sed -i '/include \/config\/nginx\/resolver\.conf;/a \    include /config/nginx/conf.d/*.conf;' "$NGINX_CONF"
    echo "→ Added conf.d include to nginx.conf"
  else
    echo "→ conf.d include already present"
  fi
else
  echo "⚠ nginx.conf not found after waiting"
fi

# ====================
# 2. proxy.conf patcher (iframe/embed support)
# ====================
PROXY_CONF="$APPDATA/nginx/proxy.conf"
echo "=== Patching proxy.conf ==="

for i in $(seq 1 120); do
  if [ -f "$PROXY_CONF" ]; then
    break
  fi
  sleep 1
done

if [ ! -f "$PROXY_CONF" ]; then
  echo "⚠ proxy.conf not found after waiting"
else
  touch "$PROXY_CONF"
  chown "$NEO_UID":"$NEO_GID" "$PROXY_CONF" || true
  chmod 0664 "$PROXY_CONF" || true

  MARKER="# neo-iframe-embed-support"

  if $NEO_SUPPORT; then
    if ! grep -q "$MARKER" "$PROXY_CONF"; then
      printf '%s\n' "$MARKER" "proxy_hide_header X-Frame-Options;" "proxy_hide_header Content-Security-Policy;" >> "$PROXY_CONF"
      echo "→ Added iframe/embed headers to proxy.conf"
    else
      echo "→ iframe/embed support already present"
    fi
  else
    sed -i "/$MARKER/d" "$PROXY_CONF" || true
    sed -i '/proxy_hide_header X-Frame-Options/d' "$PROXY_CONF" || true
    sed -i '/proxy_hide_header Content-Security-Policy/d' "$PROXY_CONF" || true
    echo "→ Removed iframe/embed headers from proxy.conf"
  fi
fi

# ====================
# 3. swag-dashboard mod default vhost
# ====================
# The mod always installs proxy-confs/dashboard.subdomain.conf when missing.
# Neo materializes the UI under ${DASHBOARD_SUBDOMAIN}.subdomain.conf instead.
echo "=== Cleaning mod-default dashboard vhost ==="
MOD_DASH_CONF="$APPDATA/nginx/proxy-confs/dashboard.subdomain.conf"
if [ "$DASHBOARD_SUBDOMAIN" != "dashboard" ] && [ -f "$MOD_DASH_CONF" ]; then
  rm -f "$MOD_DASH_CONF"
  echo "→ Removed $MOD_DASH_CONF (UI is ${DASHBOARD_SUBDOMAIN}.subdomain.conf)"
elif [ "$DASHBOARD_SUBDOMAIN" = "dashboard" ]; then
  echo "→ Subdomain is dashboard; leaving mod/Neo conf as-is"
else
  echo "→ No mod-default dashboard.subdomain.conf present"
fi
