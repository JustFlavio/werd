#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
triple="$(rustc --print host-tuple)"
if [[ "$triple" != "aarch64-apple-darwin" ]]; then
  echo "Target non supportato: $triple" >&2
  exit 1
fi
cargo build --release -p werd-core -p werd-cli
mkdir -p src-tauri/binaries
cp target/release/werd-daemon "src-tauri/binaries/werd-daemon-$triple"
cp target/release/werd "src-tauri/binaries/werd-$triple"
./node_modules/.bin/tauri build --config src-tauri/tauri.dmg.conf.json
