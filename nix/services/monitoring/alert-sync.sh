# grafana-app-alerts-sync (default.nix): put the alert rules the apps sent
# (one JSON file per source in the alerts folder) into Grafana's Apps folder
# through the provisioning API, and delete rules no app declares any more.
#
# Env (set by default.nix):
#   NEO_ALERTS_DIR   the agents' rule files
#   NEO_APPS_FOLDER  uid of the Apps folder
#   NEO_GRAFANA_API  Grafana's address inside its container

state=/run/grafana-app-alerts
# req METHOD PATH [BODY]: prints the body, then the HTTP code on the last line.
req() {
  if [ $# -ge 3 ]; then
    printf '%s' "$3" | docker exec -i grafana curl -sS -w '\n%{http_code}' -H 'Remote-User: neo' \
      -H 'Content-Type: application/json' -X "$1" --data-binary @- "$NEO_GRAFANA_API$2"
  else
    docker exec grafana curl -sS -w '\n%{http_code}' -H 'Remote-User: neo' -X "$1" "$NEO_GRAFANA_API$2"
  fi
}
code() { tail -n1 <<<"$1"; }
body() { sed '$d' <<<"$1"; }

[ "$(code "$(req GET /api/health)")" = 200 ] || {
  echo "Grafana is not up yet" >&2
  exit 1
}

r=$(req GET "/api/folders/$NEO_APPS_FOLDER")
if [ "$(code "$r")" = 404 ]; then
  r=$(req POST /api/folders "$(jq -nc --arg uid "$NEO_APPS_FOLDER" '{uid: $uid, title: "Apps"}')")
  case "$(code "$r")" in
    2* | 409 | 412) ;;
    *)
      echo "creating the Apps folder failed: $(body "$r")" >&2
      exit 1
      ;;
  esac
fi

# Valid files only, oldest first, so the newest wins for an app.
desired='{}'
# ls -tr for the order: the agents name the files <source>.json.
# shellcheck disable=SC2012
while read -r f; do
  [ -n "$f" ] || continue
  if jq -e 'type == "object"' "$f" >/dev/null 2>&1; then
    desired=$(jq -c --slurpfile add "$f" '. + $add[0]' <<<"$desired")
  else
    echo "$(basename "$f"): not a JSON object, ignored" >&2
  fi
done < <(ls -1tr "$NEO_ALERTS_DIR"/*.json 2>/dev/null || true)

r=$(req GET /api/v1/provisioning/alert-rules)
[ "$(code "$r")" = 200 ] || {
  echo "reading alert rules failed: $(body "$r")" >&2
  exit 1
}
ours=$(body "$r" | jq -c --arg f "$NEO_APPS_FOLDER" '[.[] | select(.folderUID == $f and .provenance == "api" and (.uid | startswith("app-")))]')

# Rules no app declares any more.
for uid in $(jq -r --argjson d "$desired" '($d | [.[].rules[].uid]) as $want | .[] | select([.uid] | inside($want) | not) | .uid' <<<"$ours"); do
  r=$(req DELETE "/api/v1/provisioning/alert-rules/$uid")
  case "$(code "$r")" in
    2* | 404) echo "removed alert rule $uid" ;;
    *)
      echo "removing $uid failed: $(body "$r")" >&2
      exit 1
      ;;
  esac
done

mkdir -p "$state"
for group in $(jq -r 'keys[]' <<<"$desired"); do
  grp=$(jq -c --arg g "$group" '.[$g]' <<<"$desired")
  sum=$(sha256sum <<<"$grp" | cut -d' ' -f1)
  # Unchanged and every rule still there (one may have been deleted through the API).
  have=$(jq -c --arg g "$group" '[.[] | select(.ruleGroup == $g) | .uid] | sort' <<<"$ours")
  want=$(jq -c '[.rules[].uid] | sort' <<<"$grp")
  if [ "$have" = "$want" ] && [ "$(cat "$state/$group" 2>/dev/null)" = "$sum" ]; then
    continue
  fi
  r=$(req PUT "/api/v1/provisioning/folder/$NEO_APPS_FOLDER/rule-groups/$group" "$grp")
  case "$(code "$r")" in
    2*)
      echo "$sum" >"$state/$group"
      echo "alert rules of $group: $(jq '.rules | length' <<<"$grp")"
      ;;
    *)
      echo "alert rules of $group failed: $(body "$r")" >&2
      exit 1
      ;;
  esac
done
