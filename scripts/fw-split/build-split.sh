#!/usr/bin/env bash
# Build fw-esp32c6 as a split-link image: ONE link, split by reachability
# into a bootable core and an engine, plus the loader that boots the core.
#
#   scripts/fw-split/build-split.sh <out-dir> [cargo features]
#   LP_BUILD_TAG=y scripts/fw-split/build-split.sh …   (a second build of one commit)
#
# → <out-dir>/
#     core.bin, engine.bin   the pieces the update channel serves
#     app.bin                everything for the app partition, written at 0x10000:
#                            loader · boot record · core @0x18000 · engine
#     merged.bin             a whole 4 MiB chip (bootloader, partition table,
#                            app.bin) for the emulator
#     p1/p2 .elf/.map, engine.x, p2-verify.txt   the link's evidence
#
# Layout: lp-base/lp-bootctl/src/split_layout.rs. The page size here must be
# the one the shipped IDF bootloader selects (32 KiB on the 4 MB C6); the
# core and the loader read it from the MMU and refuse anything misaligned.
set -euo pipefail

out="$(mkdir -p "$1" && cd "$1" && pwd)"
features="${2:-esp32c6,server}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
built="$repo/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6"
page=$((0x8000))
export LP_SPLIT_LINK=1

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
echo "==> pass 1 ($features${LP_BUILD_TAG:+, tag $LP_BUILD_TAG})"
link p1 "$out/engine-pass1.x"
t1=$(date +%s)
python3 "$repo/scripts/fw-split/split_reach.py" "$out/p1.elf" "$out/p1.map" --top 0 \
    --emit-ld "$out/engine.x" --json "$out/p1-split.json" | sed -n '/^{/,/^}/p'
echo "==> pass 2"
link p2 "$out/engine.x"
t2=$(date +%s)
python3 "$repo/scripts/fw-split/split_reach.py" "$out/p2.elf" "$out/p2.map" --top 0 \
    --verify-engine-base 0x42400000 | tee "$out/p2-verify.txt" | grep "== verify"
grep -q "core nodes in engine region: 0;" "$out/p2-verify.txt" || {
    echo "FAIL: pass 2 placed core code in the engine region" >&2
    exit 1
}

echo "==> loader"
( cd "$repo/lp-fw/fw-esp32c6-loader" && cargo build --release --quiet )
loader_elf="$repo/lp-fw/fw-esp32c6-loader/target/riscv32imac-unknown-none-elf/release/fw-esp32c6-loader"
cp "$loader_elf" "$out/loader.elf"
espflash save-image --chip esp32c6 --flash-size 4mb "$out/loader.elf" "$out/loader.bin" >/dev/null 2>&1

echo "==> split"
rust-objcopy --wildcard -R '.rela.*' "$out/p2.elf" "$out/p2.norel.elf"
rust-objcopy -R .engine_rodata -R .engine_text "$out/p2.norel.elf" "$out/core.elf"
rust-objcopy -O binary -j .engine_rodata -j .engine_text "$out/p2.norel.elf" "$out/engine.bin"
espflash save-image --chip esp32c6 --flash-size 4mb "$out/core.elf" "$out/core.bin" >/dev/null 2>&1

python3 "$repo/scripts/fw-split/assemble.py" --page "$page" \
    "$out/loader.bin" "$out/core.bin" "$out/engine.bin" "$out/app.bin"

"$repo/scripts/emu/build-merged-image.sh" --chip esp32c6 "$out/loader.elf" "$out/merged.bin" >/dev/null
dd if="$out/app.bin" of="$out/merged.bin" bs=4096 seek=$((0x10000 / 4096)) conv=notrunc status=none
shasum -a 256 "$out/merged.bin" | sed "s|$out/||" >"$out/merged.bin.sha256"

echo "pass 1 link: $((t1 - t0)) s, pass 2 link: $((t2 - t1)) s"
