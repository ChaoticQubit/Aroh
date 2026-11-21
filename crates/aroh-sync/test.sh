#!/bin/sh
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
kiri_home="${KIRI_HOME:-$HOME}"
HOME="$kiri_home" kiri run --rm \
  -v "$root:/src:ro" \
  -v aroh-cargo-registry:/usr/local/cargo/registry \
  -v aroh-sync-target:/target \
  -e CARGO_TARGET_DIR=/target \
  rust:slim sh -c 'mkdir -p /work && cd /src && tar --exclude=./target --exclude=./node_modules --exclude=./apps/aroh/node_modules --exclude=./.git -cf - . | tar -xf - -C /work && cd /work && cargo test --quiet -p aroh-sync --tests'
