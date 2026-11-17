# netbird-server-setup (default.nix): one attempt of the first-run setup.
# Owner and API token over the service network, network range and IPv6 off,
# this server joins, the DNS zone for its app names, token deleted. Runs once
# (marker in appdata); later changes are made in the dashboard.
#
# Env (set by default.nix):
#   NEO_APPDATA        appdata of the service
#   NEO_NETWORK        Docker network of the netbird-server container
#   NEO_OWNER_FILE     JSON body of the administrator (POST /api/setup)
#   NEO_OWNER_EMAIL    administrator e-mail (log only)
#   NEO_RANGE          network range
#   NEO_ENROL          true: this server joins the network
#   NEO_ZONE           true: DNS zone NEO_DOMAIN → this server's address
#   NEO_DOMAIN         SWAG domain (NEO_ZONE)
#   NEO_URL            public URL of the management server (NEO_ENROL)
#   NEO_HOSTNAME       NetBird name of this server, "" = its hostname (NEO_ENROL)
#   NEO_NETBIRD        the NetBird client CLI (NEO_ENROL)

state=$NEO_APPDATA/bootstrap
done_marker=$NEO_APPDATA/bootstrap-done
if [ -e "$done_marker" ]; then
  echo "netbird-server-setup: already done"
  exit 0
fi
umask 077
mkdir -p "$state"

server_ip=$(docker inspect -f "{{with index .NetworkSettings.Networks \"$NEO_NETWORK\"}}{{.IPAddress}}{{end}}" netbird-server 2>/dev/null || true)
if [ -z "$server_ip" ]; then
  echo "netbird-server-setup: container netbird-server not running yet" >&2
  exit 1
fi
base="http://$server_ip:80"
instance=$(curl -sf -m 10 "$base/api/instance") || {
  echo "netbird-server-setup: server not answering yet" >&2
  exit 1
}

if [ "$(echo "$instance" | jq -r .setup_required)" = true ]; then
  curl -sf -m 30 -X POST -H 'Content-Type: application/json' \
    --data-binary "@$NEO_OWNER_FILE" \
    "$base/api/setup" | jq -r .personal_access_token >"$state/token"
  echo "netbird-server-setup: administrator $NEO_OWNER_EMAIL created"
fi
if [ ! -s "$state/token" ]; then
  echo "netbird-server-setup: the network was set up without Neo; nothing to do"
  touch "$done_marker"
  exit 0
fi
token=$(cat "$state/token")
api() {
  local method=$1 path=$2
  shift 2
  curl -sf -m 30 -X "$method" -H "Authorization: Token $token" -H 'Content-Type: application/json' "$@" "$base$path"
}

# The API replaces the whole settings object: read, change, write back.
# Range before any device joins; no IPv6 addresses, so devices reach
# SWAG over IPv4 with their NetBird address.
account=$(api GET /api/accounts)
id=$(echo "$account" | jq -r '.[0].id')
body=$(echo "$account" | jq -c --arg r "$NEO_RANGE" \
  '{settings: (.[0].settings + {network_range: $r, ipv6_enabled_groups: []}
    | del(.embedded_idp_enabled, .local_auth_disabled, .network_range_v6))}')
got=$(api PUT "/api/accounts/$id" --data "$body" | jq -r .settings.network_range)
if [ "$got" != "$NEO_RANGE" ]; then
  echo "netbird-server-setup: network range is $got, expected $NEO_RANGE" >&2
  exit 1
fi
echo "netbird-server-setup: network range $NEO_RANGE, IPv6 off"

netbird_addr() {
  ip -4 -o addr show dev wt0 2>/dev/null | awk '{print $4}' | cut -d/ -f1 || true
}

if [ "$NEO_ENROL" = true ]; then
  # Not a pipe into grep -q: under pipefail its early exit fails the status call.
  status=$("$NEO_NETBIRD" status 2>&1 || true)
  if grep -q NeedsLogin <<<"$status"; then
    api POST /api/setup-keys --data '{"name":"neo-this-server","type":"one-off","expires_in":86400,"auto_groups":[],"usage_limit":1,"ephemeral":false}' |
      jq -r .key >"$state/setup-key"
    up=(up --setup-key-file "$state/setup-key" --management-url "$NEO_URL")
    if [ -n "$NEO_HOSTNAME" ]; then
      up+=(--hostname "$NEO_HOSTNAME")
    fi
    "$NEO_NETBIRD" "${up[@]}"
    rm -f "$state/setup-key"
  fi
  addr=
  for _ in $(seq 1 60); do
    addr=$(netbird_addr)
    [ -n "$addr" ] && break
    sleep 2
  done
  if [ -z "$addr" ]; then
    echo "netbird-server-setup: this server did not get a NetBird address" >&2
    exit 1
  fi
  echo "netbird-server-setup: this server joined as $addr"
fi

if [ "$NEO_ZONE" = true ]; then
  addr=$(netbird_addr)
  if [ -z "$addr" ]; then
    echo "netbird-server-setup: no NetBird address for the app names zone yet" >&2
    exit 1
  fi
  domain=$NEO_DOMAIN
  if ! api GET /api/dns/zones | jq -e --arg d "$domain" 'any(.[]; .domain == $d)' >/dev/null; then
    all=$(api GET /api/groups | jq -r '.[] | select(.name == "All") | .id')
    zone_id=$(api POST /api/dns/zones --data "$(jq -nc --arg d "$domain" --arg g "$all" \
      '{name: $d, domain: $d, enabled: true, enable_search_domain: false, distribution_groups: [$g]}')" | jq -r .id)
    for name in "*.$domain" "$domain"; do
      api POST "/api/dns/zones/$zone_id/records" --data "$(jq -nc --arg n "$name" --arg a "$addr" \
        '{name: $n, type: "A", content: $a, ttl: 300}')" >/dev/null
    done
    echo "netbird-server-setup: DNS zone $domain → $addr"
  fi
fi

# Nothing long-lived stays behind: delete the administrator's tokens.
for user in $(api GET /api/users | jq -r '.[] | select(.role == "owner") | .id'); do
  for t in $(api GET "/api/users/$user/tokens" | jq -r '.[].id'); do
    api DELETE "/api/users/$user/tokens/$t" >/dev/null
  done
done
rm -f "$state/token"
touch "$done_marker"
echo "netbird-server-setup: done"
