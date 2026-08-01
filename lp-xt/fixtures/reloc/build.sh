#!/usr/bin/env bash
# Assemble the two-object reloc fixtures with the esp toolchain's GNU as,
# staging .o files in fixtures/reloc/obj/ where lp-xt-elf's `reloc`-feature
# tests look for them. Also links each pair with GNU ld (oracle.ld) into a
# *behavioral oracle* executable the differential test runs via the plain
# linked-ELF loader.
#
# Golden rule (AGENTS.md): instruction bytes are assembler-derived, never
# hand-written — that is the entire point of this script.
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
for src in src/*.s; do
  name="$(basename "$src" .s)"
  "$AS" -o "obj/$name.o" "$src"
  echo "assembled obj/$name.o"
done

for fixture in mix funptr pingpong; do
  "$LD" -T oracle.ld -o "obj/$fixture.ld.elf" "obj/${fixture}_main.o" "obj/${fixture}_lib.o"
  echo "linked   obj/$fixture.ld.elf (GNU ld oracle)"
done

# Handy while iterating: show the relocations the prototype must apply.
if [[ "${1:-}" == "-v" ]]; then
  for o in obj/*_main.o obj/*_lib.o; do
    echo "=== $o ==="
    "$OBJDUMP" -r "$o"
  done
fi

echo "build.sh: OK ($(ls obj/*.o | wc -l | tr -d ' ') objects, $(ls obj/*.ld.elf | wc -l | tr -d ' ') oracles)"
