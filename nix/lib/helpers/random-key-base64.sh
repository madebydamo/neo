#!/usr/bin/env bash
# Generate a 32-byte cryptographically random key, base64 encoded (stdout only).
# For keys that must decode to exactly 32 bytes (e.g. NetBird's store encryption key).
set -euo pipefail
if command -v openssl >/dev/null 2>&1; then
  openssl rand -base64 32
elif [[ -r /dev/urandom ]]; then
  head -c 32 /dev/urandom | base64 -w0
  echo
else
  echo "no CSPRNG available" >&2
  exit 1
fi
