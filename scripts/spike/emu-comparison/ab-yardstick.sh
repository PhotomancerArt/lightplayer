#!/usr/bin/env bash
# SPIKE — same-window A/B of every emulator on the common RV32IMAC kernel.
#
#   K=<dir with yardstick-*.elf> TOOLS=<dir with the emulator binaries> \
#     scripts/spike/emu-comparison/ab-yardstick.sh
#
# Engines are run round-robin so a load spike lands on all of them. Every
# engine must print the same `checksum=` word — that is the identity oracle,
# and an engine that disagrees is dropped rather than reported.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$repo"

k="${K:?set K to the kernel dir}"
tools="${TOOLS:?set TOOLS to the dir holding rvlinux-*/rv32emu-*}"
ours="${OURS:-target/release/lp-emu-esp32c6}"
rounds="${ROUNDS:-3}"
out="${OUT:-$k/ab}"
mkdir -p "$out"

load1() { uptime | sed -E 's/.*load averages?: *([0-9.]+).*/\1/'; }

run_engine() {
    local name="$1"; shift
    local l; l="$(load1)"
    /usr/bin/time -p "$@" >"$out/$name.stdout" 2>"$out/$name.err" || true
    local real user sum
    real="$(grep -E '^real ' "$out/$name.err" | tail -1 | awk '{print $2}')"
    user="$(grep -E '^user ' "$out/$name.err" | tail -1 | awk '{print $2}')"
    sum="$(grep -oE 'checksum=[0-9a-f]+' "$out/$name.stdout" "$out/$name.err" 2>/dev/null | head -1 | cut -d= -f2)"
    printf '%-22s %-3s %8s %8s %8s  %s\n' "${name%-*}" "${name##*-}" "$user" "$real" "$l" "${sum:-NONE}"
}

printf '%-22s %-3s %8s %8s %8s  %s\n' engine rnd "user s" "wall s" load checksum
for r in $(seq 1 "$rounds"); do
    run_engine "ours-t1-$r" "$ours" --elf "$k/yardstick-c6.elf" \
        --timeout 60s --wall-timeout 600 --exit-on 'YARDSTICK DONE' \
        --uart0 stdout --time-grade t1
    run_engine "ours-t2-$r" "$ours" --elf "$k/yardstick-c6.elf" \
        --timeout 60s --wall-timeout 600 --exit-on 'YARDSTICK DONE' \
        --uart0 stdout --time-grade t2
    run_engine "ours-t1-nocache-$r" "$ours" --elf "$k/yardstick-c6.elf" \
        --timeout 60s --wall-timeout 600 --exit-on 'YARDSTICK DONE' \
        --uart0 stdout --time-grade t1 --no-block-cache
    run_engine "qemu-tcg-$r" qemu-system-riscv32 -machine virt -bios none \
        -kernel "$k/yardstick-virt.elf" -nographic -monitor none -serial mon:stdio
    run_engine "libriscv-interp-$r" "$tools/rvlinux-interp" -s "$k/yardstick-syscall.elf"
    run_engine "libriscv-bintr-$r" "$tools/rvlinux-bintr" -s "$k/yardstick-syscall.elf"
    run_engine "rv32emu-interp-$r" "$tools/rv32emu-interp" "$k/yardstick-syscall.elf"
    run_engine "rv32emu-jit-$r" "$tools/rv32emu-jit" "$k/yardstick-syscall.elf"
done
