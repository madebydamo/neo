#!/usr/bin/env bash
# Hash a password with SHA-512 crypt from JSON stdin:
#   {"password":"..."}
# stdout: $6$...
# Requires: mkpasswd (whois) or openssl. Never print the password.
set -euo pipefail
input=$(cat)

if command -v jq >/dev/null 2>&1; then
  pass=$(printf '%s' "$input" | jq -r '.password // empty')
else
  pass=$(printf '%s' "$input" | sed -n 's/.*"password"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)
fi

if [[ -z "$pass" ]]; then
  echo "password required" >&2
  exit 2
fi

if command -v mkpasswd >/dev/null 2>&1; then
  hash=$(printf '%s\n' "$pass" | mkpasswd -m sha-512 -s)
elif command -v openssl >/dev/null 2>&1; then
  hash=$(openssl passwd -6 "$pass")
else
  echo "mkpasswd or openssl required" >&2
  exit 127
fi
if [[ "$hash" != \$6\$* ]]; then
  echo "did not produce sha-512 crypt hash" >&2
  exit 1
fi
printf '%s\n' "$hash"
