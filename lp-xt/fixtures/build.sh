#!/usr/bin/env bash
# Build every fixture ELF with the esp toolchain, verify the integer-only rule
# (no FPU instructions in the disassembly), and stage the ELFs in fixtures/elf/
# where lp-xt-elf's host tests look for them.
set -euo pipefail
cd "$(dirname "$0")"

# `--if-toolchain` turns a missing esp toolchain from an error into a no-op
# exit 0, so a `just` recipe can depend on this unconditionally. Mirrors
# scripts/build-builtins-xt.sh, and carries the same warning: on a machine that
# HAS the toolchain the ELFs are rebuilt rather than assumed, because
# `fixtures/elf/` is gitignored and a fresh worktree leaves it empty — which is
# how a host test suite comes to skip and report success.
IF_TOOLCHAIN=false
if [[ "${1:-}" == "--if-toolchain" ]]; then
  IF_TOOLCHAIN=true
  shift
fi

# The GNU xtensa binutils/gcc shipped inside the rustup `esp` toolchain: the
# rust target spec links via xtensa-esp32s3-elf-gcc, so it must be on PATH.
#
# Two install shapes to find it in, as scripts/build-builtins-xt.sh explains:
# espup (developer machines) puts it under ~/.rustup/toolchains/esp; the CI
# action adds it to PATH itself. Check the espup layout first, then fall back
# to PATH, so the same script serves both.
GCC_BIN="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
if [[ -x "$GCC_BIN/xtensa-esp32s3-elf-gcc" ]]; then
  export PATH="$GCC_BIN:$PATH"
elif ! command -v xtensa-esp32s3-elf-gcc >/dev/null 2>&1; then
  if [[ "$IF_TOOLCHAIN" == true ]]; then
    echo "note: esp toolchain not installed — not building the Xtensa fixtures."
    echo "      The lp-xt-elf and lp-xt-emu mach tests will skip. Run \`espup install\`."
    exit 0
  fi
  echo "error: xtensa-esp32s3-elf-gcc not found under ~/.rustup/toolchains/esp" >&2
  echo "       and not on PATH. Install the esp toolchain (espup install) first." >&2
  exit 1
fi
OBJDUMP="$(command -v xtensa-esp32s3-elf-objdump)"
NM="$(command -v xtensa-esp32s3-elf-nm)"
if [[ -z "$NM" ]]; then
  echo "error: xtensa-esp32s3-elf-nm not found alongside the gcc above" >&2
  exit 1
fi

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
for src in corpus/src/bin/*.rs mach/src/bin/*.rs; do
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

# The `mach` fixtures are bare-metal images for the PRIVILEGED hart and carry
# xtensa-lx-rt's real vector table. Two properties the host tests depend on are
# checked here, at the source, rather than surfacing as a baffling failure in
# the emulator:
#
#  1. `.data` must be EMPTY. lx-rt's `xtensa.in.x` links `.data` with
#     `AT > RODATA`, giving it an LMA distinct from its VMA; `lp-xt-elf`'s
#     loader writes PT_LOAD segments to `p_vaddr`, so `Reset`'s `.data` copy
#     would read zeros over the real bytes. See mach/memory.x.
#  2. The vector table must be present and 1 KiB aligned at `_init_start` —
#     `Reset` does `wsr.vecbase _init_start` and VECBASE's low 10 bits are not
#     writable, so a misaligned table silently vectors somewhere else.
for src in mach/src/bin/*.rs; do
  name="$(basename "$src" .rs)"
  syms="$("$NM" "elf/$name.elf")"
  get() { echo "$syms" | awk -v s="$1" '$3 == s { print $1 }'; }
  dstart="$(get _data_start)"; dend="$(get _data_end)"
  istart="$(get _init_start)"; iend="$(get _init_end)"
  if [[ -z "$dstart" || -z "$dend" || "$dstart" != "$dend" ]]; then
    echo "error: $name has a non-empty .data ($dstart..$dend); see mach/memory.x" >&2
    exit 1
  fi
  if [[ -z "$istart" || -z "$iend" ]]; then
    echo "error: $name has no vector table (_init_start/_init_end missing)" >&2
    exit 1
  fi
  if (( 0x$istart % 0x400 != 0 )) || (( 0x$iend - 0x$istart != 0x400 )); then
    echo "error: $name vector table $istart..$iend is not a 1 KiB-aligned 0x400 block" >&2
    exit 1
  fi
done

# Count what this loop built — elf/ also holds the builtins image
# (scripts/build-builtins-xt.sh), which is not a fixture.
echo "build.sh: OK ($count fixture ELFs)"
