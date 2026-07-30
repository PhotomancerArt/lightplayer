#!/usr/bin/env bash
# Build the Xtensa builtins image — the guest-side base image `rt_emu_xt`
# links compiled shader code against (the counterpart of
# scripts/build-builtins.sh, which does the same for riscv32).
#
# Output: lp-xt/fixtures/elf/lps-builtins-xt-app.elf (gitignored; regenerable).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

# The GNU xtensa binutils/gcc shipped inside the rustup `esp` toolchain: the
# rust target spec links via xtensa-esp32s3-elf-gcc, so it must be on PATH.
GCC_BIN="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
if [[ ! -x "$GCC_BIN/xtensa-esp32s3-elf-gcc" ]]; then
  echo "error: xtensa-esp32s3-elf-gcc not found under ~/.rustup/toolchains/esp" >&2
  echo "       install the esp toolchain (espup install) first." >&2
  exit 1
fi
export PATH="$GCC_BIN:$PATH"
NM="$GCC_BIN/xtensa-esp32s3-elf-nm"

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
if ! cargo build --release -p lps-builtins-xt-app; then
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
SIZE="$(wc -c < "$OUT" | xargs)"
echo "lps-builtins-xt-app: $COUNT builtins, ${SIZE} B -> ${OUT#"$ROOT"/}"
