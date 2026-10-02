#!/usr/bin/env bash
# OTA split-link spike: build fw-esp32c6 as ONE link split into a bootable
# core image and a mappable engine blob, plus a merged 4 MiB flash image for
# the emulator with the engine at ENGINE_FLASH_OFFSET.
#
#   scripts/ota-spike/build-split.sh <out-dir> [cargo features]
#   → <out-dir>/{core.elf,core.bin,engine.bin,merged.bin,merged-no-engine.bin}
#
# Spike tooling — not product code.
set -euo pipefail

out="$(mkdir -p "$1" && cd "$1" && pwd)"
features="${2:-esp32c6,server}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
engine_flash_offset=$((0x140000)) # mirrors ENGINE_FLASH_OFFSET in main.rs
built="$repo/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6"

link() { # <pass-name> <linker script>
    touch "$repo/lp-fw/fw-esp32c6/src/main.rs" # link args alone do not dirty the crate
    ( cd "$repo/lp-fw/fw-esp32c6" && cargo rustc --quiet \
        --target riscv32imac-unknown-none-elf --profile release-esp32 \
        --features "$features" -- \
        -C link-arg=--emit-relocs -C "link-arg=-Map=$out/$1.map" -C "link-arg=-T$2" )
    cp "$built" "$out/$1.elf"
}

cat >"$out/engine-pass1.x" <<'EOF'
MEMORY {
  ENGINE : ORIGIN = 0x42400000, LENGTH = 0x400000
}
SECTIONS {
  .engine_rodata : ALIGN(4) {
    KEEP(*(.engine_header))
  } > ENGINE
}
INSERT BEFORE .rodata;
EOF

t0=$(date +%s)
echo "==> pass 1 ($features)"
link p1 "$out/engine-pass1.x"
t1=$(date +%s)
python3 "$repo/scripts/ota-spike/split_reach.py" "$out/p1.elf" "$out/p1.map" --top 0 \
    --emit-ld "$out/engine.x" --json "$out/p1-split.json" | sed -n '/^{/,/^}/p'
echo "==> pass 2"
link p2 "$out/engine.x"
t2=$(date +%s)
python3 "$repo/scripts/ota-spike/split_reach.py" "$out/p2.elf" "$out/p2.map" --top 0 \
    --verify-engine-base 0x42400000 | tee "$out/p2-verify.txt" | grep "== verify"
grep -q "core nodes in engine region: 0;" "$out/p2-verify.txt" || {
    echo "FAIL: pass 2 placed core code in the engine region" >&2
    exit 1
}

echo "==> split"
rust-objcopy --wildcard -R '.rela.*' "$out/p2.elf" "$out/p2.norel.elf"
rust-objcopy -R .engine_rodata -R .engine_text "$out/p2.norel.elf" "$out/core.elf"
rust-objcopy -O binary -j .engine_rodata -j .engine_text "$out/p2.norel.elf" "$out/engine.bin"
espflash save-image --chip esp32c6 --flash-size 4mb "$out/core.elf" "$out/core.bin" 2>/dev/null
"$repo/scripts/emu/build-merged-image.sh" --chip esp32c6 "$out/core.elf" "$out/merged-no-engine.bin" >/dev/null
cp "$out/merged-no-engine.bin" "$out/merged.bin"
dd if="$out/engine.bin" of="$out/merged.bin" bs=1 seek="$engine_flash_offset" conv=notrunc status=none

core_len=$(stat -f %z "$out/core.bin")
engine_len=$(stat -f %z "$out/engine.bin")
if (( 0x10000 + core_len > engine_flash_offset )); then
    echo "FAIL: core image ($core_len B) runs into the engine at $engine_flash_offset" >&2
    exit 1
fi
echo "core.bin   $core_len B"
echo "engine.bin $engine_len B (at flash $(printf '%#x' $engine_flash_offset))"
echo "pass 1 link: $((t1 - t0)) s, pass 2 link: $((t2 - t1)) s"
