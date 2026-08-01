#!/usr/bin/env bash
# The toolchain FP-ABI probe (M6 P4 §6). No hardware needed.
#
# Compiles abi_probe.c for the ESP32-S3 at -O3, disassembles it, and reports
# which FRs are spilled around the call — i.e. which ones the toolchain treats
# as callee-saved. binutils/GCC *output* is fact and carries no license
# obligation; their source is off limits (AGENTS.md).
#
#   ./abi_probe.sh        summary
#   ./abi_probe.sh -v     summary + the full disassembly
set -euo pipefail
cd "$(dirname "$0")"

BIN_DIR="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
GCC="$BIN_DIR/xtensa-esp32s3-elf-gcc"
OBJDUMP="$BIN_DIR/xtensa-esp32s3-elf-objdump"
if [[ ! -x "$GCC" ]]; then
  echo "NOT PROBED: xtensa-esp32s3-elf-gcc not found under ~/.rustup/toolchains/esp" >&2
  echo "            (see \`just _xt-gcc-dir\` for how to install it)" >&2
  exit 1
fi

mkdir -p obj
"$GCC" -O3 -c -o obj/abi_probe.o abi_probe.c
DISASM="$("$OBJDUMP" -d obj/abi_probe.o)"

echo "toolchain: $("$GCC" -dumpversion) ($BIN_DIR)"
echo
echo "FP register traffic around the call:"
echo "$DISASM" | grep -E '\b(ssi|ssip|ssx|ssxp|lsi|lsip|lsx|lsxp|wfr|rfr)\b' || echo "  (none)"
echo
echo "FRs written by the body:"
echo "$DISASM" | grep -oE '\bf1?[0-9]\b' | sort -u | tr '\n' ' '
echo

if [[ "${1:-}" == "-v" ]]; then
  echo "=== full disassembly ==="
  echo "$DISASM"
fi
