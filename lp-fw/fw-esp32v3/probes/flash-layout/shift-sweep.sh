#!/usr/bin/env bash
# shift-sweep.sh — relink one fw-esp32v3 image per pad size without rebuilding.
#
# Usage:
#   shift-sweep.sh LINKARGS_LOG ORDER_FILE OUT_DIR [PAD ...]
#
# LINKARGS_LOG is the output of
#   cd lp-fw/fw-esp32v3 && cargo rustc --profile release-esp32v3 -- --print link-args -C save-temps
# (`-C save-temps` keeps the LTO object in the deps dir; the link line is the
# `"xtensa-esp32-elf-gcc" …` line of that output). ORDER_FILE is one of
# order/*.x — a GNU ld --section-ordering-file that names where `.text.pad`
# goes inside `.text`. Each PAD (bytes, multiples of 32) becomes one ELF,
# OUT_DIR/elf-<name>-<pad>, differing from the plain build ONLY by that many
# bytes of dead text at the ordered position.
#
# The pad is a `.text.pad` input section made from /dev/zero with objcopy,
# carrying a global `_lp_text_pad` symbol that `-u` turns into a GC root
# (`--gc-sections` drops an unreferenced section, silently — the first
# version of this sweep produced ten identical images).
#
# Why a pad INSIDE .text and not a section before it: the classic's
# bootloader maps exactly one IROM segment; a second output section in IROM
# is a second segment, and the image dies with IllegalInstruction.
set -euo pipefail
LOG="$1"; ORDER="$2"; OUT="$3"; shift 3
name="$(basename "$ORDER" .x)"
BIN="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
export PATH="$BIN:$PATH"
xtensa-esp32-elf-ld --version | grep -q ' 2\.4[3-9]\| 2\.[5-9]' || {
  echo "xtensa-esp32-elf-ld < 2.43 has no --section-ordering-file" >&2; exit 1; }
mkdir -p "$OUT/pads"
line="$(grep -o '"xtensa-esp32-elf-gcc".*' "$LOG" | head -1)"
[[ -n "$line" ]] || { echo "no gcc link line in $LOG (build with --print link-args)" >&2; exit 1; }
for pad in "$@"; do
  head -c "$pad" /dev/zero > "$OUT/pads/pad$pad.bin"
  ( cd "$OUT/pads" && xtensa-esp32-elf-objcopy -I binary -O elf32-xtensa-le -B xtensa \
      --rename-section .data=.text.pad,alloc,load,readonly,code \
      --redefine-sym "_binary_pad${pad}_bin_start=_lp_text_pad" "pad$pad.bin" "pad$pad.o" )
  elf="$OUT/elf-$name-$pad"
  # Rewrite the recorded link line: our output path, the pad object as a GC
  # root, the ordering file, and rustc's deleted temp-dir rlib copies pointed
  # back at the originals in deps/.
  python3 - "$line" "$elf" "$OUT/pads/pad$pad.o" "$ORDER" <<'EOF' | bash
import re, shlex, sys
line, elf, pad, order = sys.argv[1:]
args = shlex.split(line)
out = []
i = 0
while i < len(args):
    a = args[i]
    if a == '-o':
        out += ['-o', elf]; i += 2; continue
    out.append(re.sub(r'/deps/rustc[A-Za-z0-9]+/lib', '/deps/lib', a)); i += 1
out += [pad, '-Wl,-u,_lp_text_pad', f'-Wl,--section-ordering-file={order}']
print(' '.join(shlex.quote(a) for a in out))
EOF
  echo "$elf: $(xtensa-esp32-elf-nm -n "$elf" | grep -E ' _lp_text_pad$| __lp_lpir_ffloor_f32$' | awk '{printf "%s=%s ", $3, $1}')"
done
