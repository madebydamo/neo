# neo-docker-network: create the Docker networks the containers name and
# remove managed ones no one declares any more (docker-network.nix).
#
# Env (set by docker-network.nix):
#   NEO_NETWORKS        lines "name|internal|subnet|gateway|bridge" (IPv4,
#                       fixed subnets first; empty field = any)
#   NEO_IPV6_NETWORK    SWAG's IPv6-only network
#   NEO_SHARED_NETWORK  `internal`: never removed
#   NEO_DECLARED        every declared network, space separated
#   NEO_LEGACY          lines: networks from before labels, removed like managed ones

exists() { docker network inspect "$1" >/dev/null 2>&1; }

# A free /24 of 172.16.0.0/12 (the range Neo reserves for Docker) that
# overlaps no Docker network and no host route. Docker only reads its
# address pool when the daemon starts. A switch restarts Docker when
# daemon.json changes, but dockerd can come back from the old unit and
# keep its old pools (about 31 networks) until the next restart
# (nix/modules/core/restart.nix), so Neo picks the subnet itself.
ip2int() {
  local IFS=. a b c d
  read -r a b c d <<<"$1"
  echo $(((a << 24) | (b << 16) | (c << 8) | d))
}
used_ranges() {
  {
    # One argument per network id.
    # shellcheck disable=SC2046
    docker network inspect -f '{{range .IPAM.Config}}{{.Subnet}} {{end}}' $(docker network ls -q)
    ip -4 route show table all | awk '$1 ~ /^[0-9.]+(\/[0-9]+)?$/ {print $1}'
  } | tr ' ' '\n' | grep -E '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+(/[0-9]+)?$' | while read -r cidr; do
    local net=${cidr%/*} len=32
    [ "$net" != "$cidr" ] && len=${cidr#*/}
    local start size
    start=$(ip2int "$net")
    size=$((1 << (32 - len)))
    start=$((start & ~(size - 1)))
    echo "$start $((start + size - 1))"
  done
}
free_subnet() {
  local used base i lo hi s e clash
  base=$(ip2int 172.16.0.0)
  used=$(used_ranges)
  for ((i = 0; i < 4096; i++)); do
    lo=$((base + i * 256))
    hi=$((lo + 255))
    clash=0
    while read -r s e; do
      [ -n "$s" ] || continue
      if ((lo <= e && s <= hi)); then
        clash=1
        break
      fi
    done <<<"$used"
    if [ "$clash" = 0 ]; then
      echo "$((lo >> 24 & 255)).$((lo >> 16 & 255)).$((lo >> 8 & 255)).0/24"
      return 0
    fi
  done
  return 1
}
in_use() { [ "$(docker network inspect -f '{{len .Containers}}' "$1")" != 0 ]; }

# ensure NAME INTERNAL(true|false) SUBNET GATEWAY BRIDGE (last three may be "")
ensure() {
  local name=$1 internal=$2 subnet=$3 gateway=$4 bridge=$5 want current
  local cur_internal cur_subnet cur_bridge
  want="$internal|$subnet|$bridge"
  if exists "$name"; then
    # Not created by Neo (internal, a plugin's own network): never touch it.
    if [ "$(docker network inspect -f '{{index .Labels "neo.managed"}}' "$name")" != 1 ]; then
      return 0
    fi
    IFS='|' read -r cur_internal cur_subnet cur_bridge < <(
      docker network inspect -f '{{.Internal}}|{{range .IPAM.Config}}{{.Subnet}}{{end}}|{{index .Options "com.docker.network.bridge.name"}}' "$name"
    )
    # An empty subnet / bridge name means "any".
    [ -z "$subnet" ] && cur_subnet=
    [ -z "$bridge" ] && cur_bridge=
    current="$cur_internal|$cur_subnet|$cur_bridge"
    [ "$current" = "$want" ] && return 0
    if in_use "$name"; then
      echo "network $name: is $current, should be $want; recreated once its containers stop" >&2
      return 0
    fi
    docker network rm "$name" >/dev/null
  fi
  local args=(--label neo.managed=1)
  [ "$internal" = true ] && args+=(--internal)
  if [ -z "$subnet" ]; then
    subnet=$(free_subnet) || {
      echo "network $name: no free /24 left in 172.16.0.0/12" >&2
      failed=1
      return 0
    }
  fi
  args+=(--subnet "$subnet")
  [ -n "$gateway" ] && args+=(--gateway "$gateway")
  [ -n "$bridge" ] && args+=(-o "com.docker.network.bridge.name=$bridge")
  if docker network create "${args[@]}" "$name" >/dev/null; then
    echo "network $name: created"
  else
    echo "network $name: could not be created ($want)" >&2
    failed=1
  fi
}
failed=0

# Fixed subnets first, before the pool hands their range to another network.
while IFS='|' read -r name internal subnet gateway bridge; do
  [ -n "$name" ] || continue
  ensure "$name" "$internal" "$subnet" "$gateway" "$bridge"
done <<<"$NEO_NETWORKS"

if ! exists "$NEO_IPV6_NETWORK"; then
  if ! docker network create --label neo.managed=1 --ipv6 --ipv4=false "$NEO_IPV6_NETWORK" >/dev/null; then
    # No IPv6 on this host. SWAG still joins the network, which then does nothing.
    echo "IPv6 network not available, SWAG gets IPv4 only" >&2
    docker network create --label neo.managed=1 "$NEO_IPV6_NETWORK" >/dev/null
  fi
fi

# Remove networks no one declares any more, once empty.
declared=" $NEO_DECLARED "
{
  docker network ls --filter label=neo.managed=1 --format '{{.Name}}'
  while read -r name; do
    if [ -n "$name" ] && exists "$name"; then echo "$name"; fi
  done <<<"$NEO_LEGACY"
} | sort -u | while read -r name; do
  [ -n "$name" ] || continue
  [ "$name" = "$NEO_SHARED_NETWORK" ] && continue
  case "$declared" in *" $name "*) continue ;; esac
  if in_use "$name"; then
    echo "network $name: no longer declared, kept while containers use it" >&2
  elif docker network rm "$name" >/dev/null 2>&1; then
    echo "network $name: removed"
  fi
done
exit "$failed"
