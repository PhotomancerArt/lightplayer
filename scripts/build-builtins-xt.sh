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

# --ignore-rust-version: the esp toolchain ships rustc 1.88-nightly while the
# workspace declares rust-version = 1.90. It is a DECLARED gate, not a real
# incompatibility — the whole builtins dependency chain compiles and links
# cleanly on 1.88 (verified). Revisit if espup ships a 1.90-based fork.
cargo build --release -p lps-builtins-xt-app --ignore-rust-version

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
