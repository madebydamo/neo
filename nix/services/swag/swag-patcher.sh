#!/usr/bin/env bash
# SWAG post-start patches (conf.d / dbip includes, iframe proxy.conf, dashboard conf cleanup).
set -uo pipefail

: "${APPDATA:=/var/neo/DATA/AppData/swag}"
: "${NEO_UID:=1000}"
: "${NEO_GID:=1000}"
: "${NEO_SUPPORT:=false}"
: "${DASHBOARD_SUBDOMAIN:=swag}"

NGINX_CONF="$APPDATA/nginx/nginx.conf"
echo "=== Patching nginx.conf ==="

# SWAG copies nginx.conf from the image after the container is already
# "started". Patching too early is a silent no-op (oneshot RemainAfterExit),
# so $lan-ip from dbip.conf never exists. Wait for the insertion point.
nginx_conf_ready() {
  [ -f "$NGINX_CONF" ] && grep -qE 'include[[:space:]]+/config/nginx/resolver\.conf;' "$NGINX_CONF"
}

for i in $(seq 1 180); do
  if nginx_conf_ready; then
    break
  fi
  sleep 1
done

if ! nginx_conf_ready; then
  echo "⚠ nginx.conf not ready after waiting (no resolver.conf include)" >&2
  exit 1
fi

if ! grep -qE '^[[:space:]]*include[[:space:]]+/config/nginx/conf\.d/\*\.conf;' "$NGINX_CONF"; then
  sed -i '/include \/config\/nginx\/resolver\.conf;/a \    include /config/nginx/conf.d/*.conf;' "$NGINX_CONF"
  echo "→ Added conf.d include"
else
  echo "→ conf.d include already present"
fi
if ! grep -qE '^[[:space:]]*include[[:space:]]+/config/nginx/dbip\.conf;' "$NGINX_CONF"; then
  sed -i '/include \/config\/nginx\/resolver\.conf;/a \    include /config/nginx/dbip.conf;' "$NGINX_CONF"
  echo "→ Added dbip.conf include"
else
  echo "→ dbip.conf include already present"
fi

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
      echo "→ Added iframe/embed headers"
    else
      echo "→ iframe/embed support already present"
    fi
  else
    sed -i "/$MARKER/d" "$PROXY_CONF" || true
    sed -i '/proxy_hide_header X-Frame-Options/d' "$PROXY_CONF" || true
    sed -i '/proxy_hide_header Content-Security-Policy/d' "$PROXY_CONF" || true
    echo "→ Removed iframe/embed headers"
  fi
fi

echo "=== Cleaning mod-default dashboard vhost ==="
MOD_DASH_CONF="$APPDATA/nginx/proxy-confs/dashboard.subdomain.conf"
if [ "$DASHBOARD_SUBDOMAIN" != "dashboard" ] && [ -f "$MOD_DASH_CONF" ]; then
  rm -f "$MOD_DASH_CONF"
  echo "→ Removed $MOD_DASH_CONF"
elif [ "$DASHBOARD_SUBDOMAIN" = "dashboard" ]; then
  echo "→ Subdomain is dashboard; leaving conf as-is"
else
  echo "→ No mod-default dashboard.subdomain.conf present"
fi

echo "=== Reloading nginx ==="
if command -v docker >/dev/null 2>&1 && docker exec swag test -f /config/nginx/nginx.conf 2>/dev/null; then
  if docker exec swag nginx -c /config/nginx/nginx.conf -t 2>/dev/null \
    && docker exec swag nginx -c /config/nginx/nginx.conf -s reload 2>/dev/null; then
    echo "→ nginx reloaded"
  else
    echo "⚠ nginx not ready for reload (entrypoint may still be starting)"
  fi
else
  echo "→ skip reload (swag container not running)"
fi
