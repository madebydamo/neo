#!/usr/bin/env bash
# Generate a 32-byte cryptographically random hex secret (stdout only).
set -euo pipefail
if command -v openssl >/dev/null 2>&1; then
  openssl rand -hex 32
elif [[ -r /dev/urandom ]]; then
  head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n'
  echo
else
  echo "no CSPRNG available" >&2
  exit 1
fi
