#!/usr/bin/env bash
# Build the comms lab's wasm host half into spikes/link-lab-wasm/pkg/
# (gitignored), which spikes/serial-lab and spikes/ble-lab serve at /pkg/.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir "$here/pkg" \
    "$here/target/wasm32-unknown-unknown/release/link_lab_wasm.wasm"
ls -la "$here/pkg"
