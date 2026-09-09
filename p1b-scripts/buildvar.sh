#!/bin/bash
# $1 = tag, $2 = feature list (may be empty)
set -e
cd /Users/yona/dev/photomancer/lp2025/.claude/worktrees/agent-a1676bc71e062fc30
TAG="$1"; FEAT="$2"
NODBG=(--config profile.release.package.lp-emu-core.debug=0
       --config profile.release.package.lp-riscv-emu.debug=0
       --config profile.release.package.lp-emu-esp-common.debug=0
       --config profile.release.package.lp-emu-esp32c6.debug=0
       --config profile.release.package.lp-xt-emu.debug=0)
if [ -n "$FEAT" ]; then FA=(--features "$FEAT"); else FA=(); fi
cargo build -p lp-emu-esp32c6 --release --bin lp-emu-esp32c6 "${FA[@]}" 2>&1 | tail -1
cp target/release/lp-emu-esp32c6 "target/p1b/$TAG"
CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+bulk-memory,+simd128,+nontrapping-fptoint" \
  cargo build -p lp-emu-esp32c6 --bin lp-emu-esp32c6 --release --target wasm32-wasip1 \
  "${FA[@]}" "${NODBG[@]}" 2>&1 | tail -1
cp target/wasm32-wasip1/release/lp-emu-esp32c6.wasm "target/p1b/$TAG.wasm"
ls -la "target/p1b/$TAG" "target/p1b/$TAG.wasm"
