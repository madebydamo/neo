#!/usr/bin/env bash
set -euo pipefail

echo 'Building NixOS Docker image...'
nix build

echo 'Loading into Docker...'
xz -d -c result/tarball/nixos-system-x86_64-linux.tar.xz | docker import - homeserver:latest
echo "Tagged as homeserver:latest"

docker compose up -d --remove-orphans
