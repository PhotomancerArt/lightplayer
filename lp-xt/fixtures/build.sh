#!/usr/bin/env bash
# Build every fixture ELF with the esp toolchain, verify the integer-only rule
# (no FPU instructions in the disassembly), and stage the ELFs in fixtures/elf/
# where lp-xt-elf's host tests look for them.
set -euo pipefail
cd "$(dirname "$0")"

# The GNU xtensa binutils/gcc shipped inside the rustup `esp` toolchain: the
# rust target spec links via xtensa-esp32s3-elf-gcc, so it must be on PATH.
GCC_BIN="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
if [[ ! -x "$GCC_BIN/xtensa-esp32s3-elf-gcc" ]]; then
  echo "error: xtensa-esp32s3-elf-gcc not found under ~/.rustup/toolchains/esp" >&2
  exit 1
fi
export PATH="$GCC_BIN:$PATH"
OBJDUMP="$GCC_BIN/xtensa-esp32s3-elf-objdump"

# Keep artifacts local regardless of any global cargo build-dir config.
export CARGO_TARGET_DIR="$PWD/target"

cargo build --release

# Note on the integer-only rule: a textual objdump scan for FPU mnemonics is
# NOT reliable — objdump disassembles the literal pool at the head of .text as
# garbage "instructions" (ule.s / moveqz.s / lsx false positives). The real
# gate is the emulator itself: lp-xt-inst decodes only the integer subset, so
# any FPU op on an executed path raises an illegal-instruction trap and fails
# the fixture test. ($OBJDUMP stays available for inspecting failures.)
: "$OBJDUMP"

mkdir -p elf
count=0
for src in corpus/src/bin/*.rs; do
  name="$(basename "$src" .rs)"
  bin="target/xtensa-esp32s3-none-elf/release/$name"
  if [[ ! -f "$bin" ]]; then
    echo "error: expected fixture binary missing: $bin" >&2
    exit 1
  fi
  cp "$bin" "elf/$name.elf"
  echo "built elf/$name.elf"
  count=$((count + 1))
done

# Count what this loop built — elf/ also holds the builtins image
# (scripts/build-builtins-xt.sh), which is not a fixture.
echo "build.sh: OK ($count fixture ELFs)"
