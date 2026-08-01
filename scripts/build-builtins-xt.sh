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
# The native-f32 builtin family stays **opt-in**, for a different reason than
# when this gate was introduced.
#
# It was originally opt-in because `lps-builtins/float-f32` did not compile for
# Xtensa at all — the backend cannot select a float constant pool. That is
# fixed on our side (see the workaround and its bit-equivalence test in
# lps-builtins' rgb2hsv_f32.rs); the image now links with the family in.
#   docs/defects/2026-08-01-xtensa-backend-cannot-select-float-constant-pool.md
#
# What it runs into instead is **capacity**. link.ld gives .text 112 KiB of the
# emulator's 128 KiB code region; with float-f32 .text is 113,757 B, so the
# image links with ~931 bytes to spare — and `rt_emu::xt_image` places compiled
# shader code in exactly that gap. Every filetest whose shader exceeds it fails
# to link: the xtn.q32 suite drops from 849/849 files to 522/849. Measured, not
# predicted; the two images are 66,300 B and 113,757 B of .text.
#   docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md
#
# This is load-bearing beyond the f32 path: scripts/filetests.sh builds this
# image before running the xtn/xtlpn targets, so anything wrong here takes the
# whole Xtensa filetest suite down with it. Hence opt-in until the region/split
# question is decided — `LP_XT_BUILTINS_F32=1` requests the family for the
# host-execution work that needs it (see lpvm-native's xt_pipeline_f32.rs).
F32_REQUESTED="${LP_XT_BUILTINS_F32:-0}"
if [[ "$F32_REQUESTED" == "1" ]]; then
  BUILD_CMD=(cargo build --release -p lps-builtins-xt-app --features float-f32)
else
  BUILD_CMD=(cargo build --release -p lps-builtins-xt-app)
fi

if ! "${BUILD_CMD[@]}"; then
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
# The f32 family is what M7's hardware-float lowering calls for everything it
# does not inline — divide, sqrt, the rounding family, min/max, the saturating
# conversions and every transcendental (M7 D4). An image without those symbols
# cannot resolve any of it, so the count is reported either way and asserted
# when the family was actually requested (mirroring the rv32 script, where a
# missing family fails 800+ filetests with one opaque message).
F32_COUNT="$("$NM" "$OUT" | grep -c '_f32$' || true)"
if [[ "$F32_REQUESTED" == "1" && "$F32_COUNT" -eq 0 ]]; then
  echo "error: $OUT contains no native-f32 builtins despite --features float-f32" >&2
  exit 1
fi
SIZE="$(wc -c < "$OUT" | xargs)"
echo "lps-builtins-xt-app: $COUNT builtins ($F32_COUNT native-f32), ${SIZE} B -> ${OUT#"$ROOT"/}"
