#!/usr/bin/env bash
# Flash cost of lp-link on riscv32imac (the ESP32-C6's ISA): builds the size
# probe once per variant (LTO, opt-level z) and prints .text/.rodata and the
# delta against a baseline that already carries alloc and core::fmt. Also
# prints size_of::<Link<_>>() on the target and the biggest lp-link symbols.
set -euo pipefail
cd "$(dirname "$0")"
out=../../../target/lp-link-size
target=riscv32imac-unknown-none-elf

sizes() {
    rust-size -A "$1" | awk '$1==".text"{t=$2} $1==".rodata"{r=$2} END{print t+0, r+0}'
}

variants=(base noarq sw gbn sr sr,crc16 crypto sr-secure)
for v in "${variants[@]}"; do
    cargo build -q --release --target "$target" --features "$v" --target-dir "$out"
    cp "$out/$target/release/lp-link-size-probe" "$out/probe-$v"
done

read -r base_text base_ro < <(sizes "$out/probe-base")
printf '| variant | .text | .rodata | flash over baseline |\n|---|---|---|---|\n'
for v in "${variants[@]}"; do
    read -r t r < <(sizes "$out/probe-$v")
    printf '| %s | %d | %d | %d |\n' "$v" "$t" "$r" $((t + r - base_text - base_ro))
done

echo
echo "size_of::<Link<_>>() on $target (bytes):"
rust-nm -S -t d "$out/probe-sr" | awk '/LINK_STRUCT_SIZE/ {printf "  %s %d\n", $4, $2}'
rust-nm -S -t d "$out/probe-gbn" | awk '/LINK_STRUCT_SIZE/ {printf "  %s %d\n", $4, $2}'
rust-nm -S -t d "$out/probe-sr-secure" | awk '/LINK_STRUCT_SIZE/ {printf "  %s %d\n", $4, $2}'

echo
echo "largest symbols in the selective-repeat probe:"
rust-nm -S -t d --size-sort --demangle "$out/probe-sr" | tail -25 | awk '{printf "  %6d %s\n", $2, substr($0, index($0,$4))}'
