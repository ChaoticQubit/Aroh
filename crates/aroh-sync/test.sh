#!/bin/sh
set -eu
crate="$(cd "${1:-$(dirname "$0")}" && pwd)"
kiri_home="${KIRI_HOME:-$HOME}"
HOME="$kiri_home" kiri run --rm \
  -v "$crate:/src:ro" \
  -v aroh-cargo-registry:/usr/local/cargo/registry \
  -v aroh-sync-target:/target \
  -e CARGO_TARGET_DIR=/target \
  rust:slim sh -c 'rm -rf /work && cp -r /src /work && cd /work && cargo test --quiet'
