#!/usr/bin/env bash
# The LAST whole `[OUT] dump` in a console on stdin, as one hex string.
#
# `frame_dump` prints a frame's full dump as `part=i/n` lines of
# `DUMP_LEDS_PER_PART` LEDs each (a log record on an lp-link board is cut at
# 200 bytes), so a reader joins the parts' `rgb=` in order and keeps the last
# dump whose parts all arrived. `grep -o` rather than whole lines because a
# console can interleave (the classic prints straight into its UART FIFO), and
# `-a` because an lp-link console holds frames around the text.
#
#   scripts/frame-dump-hex.sh < console.txt          # the hex, or nothing
#   scripts/frame-dump-hex.sh --count < console.txt  # how many dumps began
#
# Shared by scripts/m4-hardware-walk.sh, scripts/emu/m4-walk.sh and
# scripts/emu/m4-walk-esp32v3.sh; the line shape is
# lp-fw/fw-esp32*/src/output/rmt/frame_dump.rs's, held equal across the three
# chips by lp-fw/fw-tests/tests/frame_dump_parity.rs.
set -uo pipefail

pattern='dump frame=[0-9]* leds=[0-9]* shown=[0-9]* crc=0x[0-9a-f]* part=[0-9]*/[0-9]* rgb=[0-9a-f]*'

if [[ "${1:-}" == "--count" ]]; then
    grep -ao "$pattern" | grep -c ' part=1/' || true
    exit 0
fi

grep -ao "$pattern" | awk '
    {
        split($6, p, "[=/]")
        hex = $7
        sub(/^rgb=/, "", hex)
        if (p[2] == 1) { cur = hex; next_part = 2 }
        else if (p[2] == next_part) { cur = cur hex; next_part++ }
        else { cur = ""; next_part = -1 }
        if (p[2] == p[3] && next_part == p[3] + 1) { last = cur }
    }
    END { printf "%s", last }
' || true
