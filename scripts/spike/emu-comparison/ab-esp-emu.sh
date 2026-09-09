#!/usr/bin/env bash
# Same-window A/B: our lp-emu-esp32c6 vs Espressif's esp-emu 0.42.0, on the
# SAME firmware image, to the SAME guest milestone.
#
#   ESP_EMU=<path to esp-emu> BINS=<dir with *.bin> scripts/spike/emu-comparison/ab-esp-emu.sh
#
# Alternates ours / theirs round by round so a load spike lands on both sides.
# Reports USER cpu seconds (the only figure that survives a busy desk) and the
# 1-minute load average next to every run.
#
# SPIKE CODE — throwaway. Nothing here gates anything.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$repo"

ours="${OURS:-target/release/lp-emu-esp32c6}"
esp_emu="${ESP_EMU:?set ESP_EMU to the esp-emu binary}"
bins="${BINS:?set BINS to the dir holding harness.bin/render-basic.bin/render-rocaille.bin}"
rounds="${ROUNDS:-3}"
out="${OUT:-$bins/ab}"
mkdir -p "$out"

# slug | our ELF | emulated timeout | exit-on
images=(
    "harness|target/emu-ref/d6cfaa205-harness/fw-esp32c6|5s|=== DONE ==="
    "render-basic|target/emu-ref/8ffc4b325-render-basic/fw-esp32c6|8s|[render-loop] === DONE ==="
    "render-rocaille|target/emu-ref/8ffc4b325-render-rocaille/fw-esp32c6|8s|[render-loop] === DONE ==="
)

load1() { uptime | sed -E 's/.*load averages?: *([0-9.]+).*/\1/'; }

# $1 label  $2.. command — prints "user_s wall_s load"
timed() {
    local label="$1"; shift
    local t="$out/$label.time"
    local l; l="$(load1)"
    /usr/bin/time -p "$@" >"$out/$label.stdout" 2>"$t.raw" || true
    # /usr/bin/time -p writes real/user/sys as the LAST three lines of stderr
    local real user
    real="$(grep -E '^real ' "$t.raw" | tail -1 | awk '{print $2}')"
    user="$(grep -E '^user ' "$t.raw" | tail -1 | awk '{print $2}')"
    printf '%s %s %s\n' "$user" "$real" "$l"
}

printf '%-16s %-6s %-8s %8s %8s %8s\n' image side round "user s" "wall s" load
for spec in "${images[@]}"; do
    IFS='|' read -r slug elf timeout exit_on <<<"$spec"
    for r in $(seq 1 "$rounds"); do
        read -r u w l <<<"$(timed "$slug-ours-t1-$r" \
            "$ours" --elf "$elf" --timeout "$timeout" --wall-timeout 600 \
            --exit-on "$exit_on" --uart0 "file:$out/$slug-ours-t1-$r.uart" \
            --time-grade t1)"
        printf '%-16s %-6s %-8s %8s %8s %8s\n' "$slug" ours-t1 "$r" "$u" "$w" "$l"

        read -r u w l <<<"$(timed "$slug-ours-t2-$r" \
            "$ours" --elf "$elf" --timeout "$timeout" --wall-timeout 600 \
            --exit-on "$exit_on" --uart0 "file:$out/$slug-ours-t2-$r.uart" \
            --time-grade t2)"
        printf '%-16s %-6s %-8s %8s %8s %8s\n' "$slug" ours-t2 "$r" "$u" "$w" "$l"

        read -r u w l <<<"$(timed "$slug-esp-$r" \
            "$esp_emu" --chip esp32c6 --firmware "$bins/$slug.bin" \
            --timeout 900s --exit-on "$exit_on" --log-color never)"
        printf '%-16s %-6s %-8s %8s %8s %8s\n' "$slug" esp-emu "$r" "$u" "$w" "$l"
    done
done
