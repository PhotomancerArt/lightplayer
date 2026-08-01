#!/usr/bin/env bash
# Build the Xtensa builtins image — the guest-side base image `rt_emu_xt`
# links compiled shader code against (the counterpart of
# scripts/build-builtins.sh, which does the same for riscv32).
#
# Output: lp-xt/fixtures/elf/lps-builtins-xt-app.elf (gitignored; regenerable).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

# The GNU xtensa binutils/gcc shipped with the esp toolchain: the rust target
# spec links via xtensa-esp32s3-elf-gcc, so it must be on PATH.
#
# Two install shapes to find it in. espup (developer machines) puts it under
# ~/.rustup/toolchains/esp/; the esp-rs/xtensa-toolchain CI action puts it
# somewhere of its own choosing and adds it to PATH. Check the espup layout
# first, then fall back to whatever is already on PATH, so the same script
# serves both.
GCC_BIN="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
if [[ -x "$GCC_BIN/xtensa-esp32s3-elf-gcc" ]]; then
  export PATH="$GCC_BIN:$PATH"
elif ! command -v xtensa-esp32s3-elf-gcc >/dev/null 2>&1; then
  echo "error: xtensa-esp32s3-elf-gcc not found under ~/.rustup/toolchains/esp" >&2
  echo "       and not on PATH. Install the esp toolchain (espup install) first." >&2
  exit 1
fi
NM="$(command -v xtensa-esp32s3-elf-nm)"
if [[ -z "$NM" ]]; then
  echo "error: xtensa-esp32s3-elf-nm not found alongside the gcc above" >&2
  exit 1
fi

OUT_DIR="$ROOT/lp-xt/fixtures/elf"
OUT="$OUT_DIR/lps-builtins-xt-app.elf"
BUILT="$ROOT/lp-xt/fixtures/target/xtensa-esp32s3-none-elf/release/lps-builtins-xt-app"

cd "$ROOT/lp-xt/fixtures"
export CARGO_TARGET_DIR="$PWD/target"

# No --ignore-rust-version: esp Rust >= 1.90 satisfies the workspace MSRV, and
# 1.90.0.0 shipped 2025-09. Overriding the check would hide a stale toolchain,
# and staleness is not harmless here — on esp Rust 1.88 `lpc-model` genuinely
# fails to compile (70x E0716 from the Slotted derive's const-promotion of a
# temporary), which is what blocked the firmware crates. Fail loudly instead.
if ! cargo build --release -p lps-builtins-xt-app --features float-f32; then
  echo >&2
  echo "error: build failed. If that was an MSRV error, the esp toolchain is stale:" >&2
  echo "       installed: $(rustc +esp --version 2>/dev/null || echo unknown)" >&2
  echo "       fix:       espup update   (needs esp Rust >= 1.90)" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
cp "$BUILT" "$OUT"

# The image's entire purpose is to carry the builtin symbols; an image that
# links but lost them to dead-code elimination is useless and would fail far
# away from here. Assert rather than trust.
COUNT="$("$NM" "$OUT" | grep -c '__lps_' || true)"
if [[ "$COUNT" -eq 0 ]]; then
  echo "error: $OUT contains no __lps_ symbols (dead-code elimination?)" >&2
  exit 1
fi
# The f32 family is the reason M7 exists on this image: hardware-float lowering
# routes divide, sqrt, the rounding family, min/max and every transcendental to
# these symbols (M7 D4). An image built without `float-f32` links none of them
# and every such call fails to resolve — checked by name, mirroring the rv32
# script, because the downstream failure is opaque.
F32_COUNT="$("$NM" "$OUT" | grep -c '_f32$' || true)"
if [[ "$F32_COUNT" -eq 0 ]]; then
  echo "error: $OUT contains no native-f32 builtins (is --features float-f32 set?)" >&2
  exit 1
fi
SIZE="$(wc -c < "$OUT" | xargs)"
echo "lps-builtins-xt-app: $COUNT builtins ($F32_COUNT native-f32), ${SIZE} B -> ${OUT#"$ROOT"/}"
