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
READELF="$(command -v xtensa-esp32s3-elf-readelf)"
if [[ -z "$READELF" ]]; then
  echo "error: xtensa-esp32s3-elf-readelf not found alongside the gcc above" >&2
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
#
# The native-f32 builtin family is **unconditional**. It was opt-in twice, for
# two different reasons, and both are now closed:
#
# 1. `lps-builtins/float-f32` did not compile for Xtensa at all — the backend
#    cannot select a float constant pool. Worked around on our side, with a
#    bit-equivalence test, in lps-builtins' rgb2hsv_f32.rs.
#      docs/defects/2026-08-01-xtensa-backend-cannot-select-float-constant-pool.md
#      upstream: https://github.com/esp-rs/rust/issues/282
# 2. It then did not *fit*: link.ld gave .text 112 KiB of the emulator's 128 KiB
#    code region and `rt_emu::xt_image` placed compiled shader code in whatever
#    was left after it — 931 bytes with float-f32 in, which dropped the xtn.q32
#    suite from 849/849 files to 522/849. That was an artifact of modeling
#    flash-resident firmware as if it lived in SRAM. It doesn't, and now it
#    doesn't here either: the image links into IROM/DROM, and the whole SRAM
#    code region is the shader's.
#      docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md
#
# This is load-bearing beyond the f32 path: scripts/filetests.sh builds this
# image before running the xtn/xtlpn targets, so anything wrong here takes the
# whole Xtensa filetest suite down with it.
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
# The f32 family is what M7's hardware-float lowering calls for everything it
# does not inline — divide, sqrt, the rounding family, min/max, the saturating
# conversions and every transcendental (M7 D4). An image without those symbols
# cannot resolve any of it (mirroring the rv32 script, where a missing family
# fails 800+ filetests with one opaque message).
F32_COUNT="$("$NM" "$OUT" | grep -c '_f32$' || true)"
if [[ "$F32_COUNT" -eq 0 ]]; then
  echo "error: $OUT contains no native-f32 builtins despite --features float-f32" >&2
  exit 1
fi

# The image is firmware: .text executes from flash (IROM), .rodata is read from
# flash (DROM), and neither may land in the SRAM code region the JIT owns. A
# link.ld edit that put them back would otherwise surface as a loader error far
# from here, so check the segment addresses at the source.
SEGS="$("$READELF" -lW "$OUT" | awk '$1 == "LOAD" { print $3 }')"
for VADDR in $SEGS; do
  case "$VADDR" in
    0x42*|0x3c0*|0x3C0*|0x3fca*|0x3FCA*|0x3fcb*|0x3FCB*) ;;
    *)
      echo "error: $OUT has a PT_LOAD segment at $VADDR, outside the modeled" >&2
      echo "       flash windows (IROM 0x42000000, DROM 0x3c000000) and image" >&2
      echo "       DRAM (0x3fca8000). Check lp-xt/lps-builtins-xt-app/link.ld." >&2
      exit 1
      ;;
  esac
done
SIZE="$(wc -c < "$OUT" | xargs)"
echo "lps-builtins-xt-app: $COUNT builtins ($F32_COUNT native-f32), ${SIZE} B -> ${OUT#"$ROOT"/}"
