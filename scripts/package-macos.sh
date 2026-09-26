#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
triple="$(rustc --print host-tuple)"
if [[ "$triple" != "aarch64-apple-darwin" ]]; then
  echo "Unsupported target: $triple" >&2
  exit 1
fi
cargo build --release -p werd-core -p werd-cli -p werd-shim
mkdir -p src-tauri/binaries
cp target/release/werd-daemon "src-tauri/binaries/werd-daemon-$triple"
cp target/release/werd "src-tauri/binaries/werd-$triple"
cp target/release/werd-shim "src-tauri/binaries/werd-shim-$triple"
./node_modules/.bin/tauri build --config src-tauri/tauri.dmg.conf.json
