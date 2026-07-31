#!/usr/bin/env bash
# Assemble the FP-subset fixtures with the esp toolchain's GNU as, staging the
# results in fixtures/fp/obj/ for the lp-xt-inst objdiff rig.
#
# Golden rule (AGENTS.md): instruction bytes are assembler-derived, never
# hand-written — that is the entire point of this script. binutils' *output* is
# fact and carries no license obligation; its source is off limits.
#
#   ./build.sh          assemble + link
#   ./build.sh -v       also dump the disassembly (this is where goldens come
#                       from when a new instruction is added to the subset)
set -euo pipefail
cd "$(dirname "$0")"

BIN_DIR="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
AS="$BIN_DIR/xtensa-esp32s3-elf-as"
LD="$BIN_DIR/xtensa-esp32s3-elf-ld"
OBJDUMP="$BIN_DIR/xtensa-esp32s3-elf-objdump"
if [[ ! -x "$AS" ]]; then
  echo "error: xtensa-esp32s3-elf-as not found under ~/.rustup/toolchains/esp" >&2
  exit 1
fi

mkdir -p obj
for src in *.S; do
  name="$(basename "$src" .S)"
  "$AS" -o "obj/$name.o" "$src"
  # objdiff wants a linked ELF with a .text at a real address, not a relocatable.
  "$LD" -e 0 -Ttext 0x40080000 -o "obj/$name.elf" "obj/$name.o"
  echo "assembled obj/$name.o -> obj/$name.elf"
done

if [[ "${1:-}" == "-v" ]]; then
  for o in obj/*.elf; do
    echo "=== $o ==="
    "$OBJDUMP" -d "$o"
  done
fi

echo "build.sh: OK ($(ls obj/*.elf | wc -l | tr -d ' ') ELFs)"
