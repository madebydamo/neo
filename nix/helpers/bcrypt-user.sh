#!/usr/bin/env bash
# Create a tinyauth username:bcrypt_hash line from JSON stdin:
#   {"username":"alice","password":"..."}
# stdout: alice:$2y$...  (or $2a$/$2b$ depending on tool)
# Prefers: jq + htpasswd; falls back to sed + mkpasswd (whois).
# Never prints the password.
set -euo pipefail

input=$(cat)

json_field() {
  local key="$1"
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "$input" | jq -r --arg k "$key" '.[$k] // empty'
  else
    # Minimal fallback for flat {"key":"value"} objects from neo-web.
    printf '%s' "$input" | sed -n "s/.*\"${key}\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" | head -n1
  fi
}

user=$(json_field username)
pass=$(json_field password)

if [[ -z "$user" || -z "$pass" ]]; then
  echo "username and password required" >&2
  exit 2
fi
if [[ "$user" == *:* ]]; then
  echo "username must not contain ':'" >&2
  exit 2
fi

hash_line=""
if command -v htpasswd >/dev/null 2>&1; then
  hash_line=$(htpasswd -nbB "$user" "$pass")
elif command -v mkpasswd >/dev/null 2>&1; then
  hash=$(printf '%s\n' "$pass" | mkpasswd -m bcrypt -s)
  hash_line="${user}:${hash}"
else
  echo "neither htpasswd nor mkpasswd found on PATH" >&2
  exit 127
fi

if [[ "$hash_line" != *:'$2'* ]]; then
  echo "helper did not produce bcrypt" >&2
  exit 1
fi
printf '%s\n' "$hash_line"
