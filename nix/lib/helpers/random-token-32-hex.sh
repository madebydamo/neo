#!/usr/bin/env bash
# Generate a 16-byte cryptographically random hex secret (32 hex characters, stdout only).
# Suitable for Activepieces AP_ENCRYPTION_KEY and similar AES-128 key material.
set -euo pipefail
if command -v openssl >/dev/null 2>&1; then
  openssl rand -hex 16
elif [[ -r /dev/urandom ]]; then
  head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n'
  echo
else
  echo "no CSPRNG available" >&2
  exit 1
fi
