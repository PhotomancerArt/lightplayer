#!/usr/bin/env bash
# SPIKE — build the yardstick kernel for each platform shim.
#
#   YARDSTICK_ITERS=200 scripts/spike/emu-comparison/build-kernel.sh <outdir>
#
# One kernel, three shims. The compute function is the same rv32imac codegen in
# all three; only the console/exit tail differs (a few dozen instructions).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="${1:?usage: build-kernel.sh <outdir>}"
mkdir -p "$out"
iters="${YARDSTICK_ITERS:-200}"
target=riscv32imac-unknown-none-elf

cd "$here/kernel"
for plat in c6 virt syscall; do
    RUSTFLAGS="-C link-arg=-T$here/kernel/link-$plat.ld -C link-arg=--no-rosegment" \
    YARDSTICK_ITERS="$iters" \
        cargo build --release --target "$target" \
        --no-default-features --features "plat-$plat" \
        --target-dir "target/$plat" >/dev/null
    cp "target/$plat/$target/release/rv-yardstick" "$out/yardstick-$plat.elf"
    echo "$out/yardstick-$plat.elf  $(shasum -a 256 "$out/yardstick-$plat.elf" | cut -c1-16)"
done
echo "iters=$iters"
