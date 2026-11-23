# The machines Monitoring expects to hear from (monitoring/default.nix,
# monitoring-expected-hosts): one per agent file in agents/, which every
# monitoring agent sends (monitoring-agent/default.nix). The file stays when a
# machine goes down, so "Server stopped reporting" keeps firing until it
# reports again or someone deletes its file.
#
# Env:
#   NEO_AGENTS_DIR   agents/ in Monitoring's appdata (<source>.json, file_sd format)
#   NEO_METRICS_OUT  node exporter textfile to write (neo_monitoring_expected_host)

out=$NEO_METRICS_OUT
{
  echo "# HELP neo_monitoring_expected_host A machine whose monitoring agent reported to this server; it is expected to keep reporting."
  echo "# TYPE neo_monitoring_expected_host gauge"
  for f in "$NEO_AGENTS_DIR"/*.json; do
    [ -e "$f" ] || continue
    # The machine's host label, as its agent sends it. Only safe characters:
    # the value goes into a label without escaping.
    host=$(jq -r 'first(.[]? | .labels.app_host? // empty | strings | select(test("^[A-Za-z0-9._-]{1,128}$"))) // empty' "$f" 2>/dev/null || true)
    if [ -z "$host" ]; then
      echo "skipping $f: no valid app_host label" >&2
      continue
    fi
    echo "neo_monitoring_expected_host{expected_host=\"$host\"} 1"
  done | sort -u
} >"$out.tmp"
mv -f "$out.tmp" "$out"
