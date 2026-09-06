#!/usr/bin/env bash
# mask-transcript.sh <uart.bin|txt>  → stdout: ANSI stripped, CRs dropped, heap digits masked to N
# Masks: "NNN B free / NNN B used (NNNk / NNNk)", "NNNk free / NNNk used", freeBytes/usedBytes/largestFreeBlock,
# uptime_ms, elapsed=NNms, frame=/fps=/tick=, boot-time "I (NNN)", byte counts in heartbeat timing fields.
sed -e 's/\x1b\[[0-9;]*m//g' -e 's/\r//g' "$1" \
 | sed -E \
   -e 's/[0-9]+ B free \/ [0-9]+ B used \([0-9]+k \/ [0-9]+k\)/N B free \/ N B used (Nk \/ Nk)/g' \
   -e 's/[0-9]+k free \/ [0-9]+k used/Nk free \/ Nk used/g' \
   -e 's/"(freeBytes|usedBytes|largestFreeBlock|uptime_ms|frame_count|frame_num|frame_delta_ms|frame_total_ms|last_frame_time_us|theoretical_fps)":[0-9.]+/"\1":N/g' \
   -e 's/"(avg|min|max)":[0-9.]+/"\1":N/g' \
   -e 's/(elapsed|frame|fps|recv|tick|send|total)=[0-9]+(ms)?/\1=N\2/g' \
   -e 's/high-water [0-9]+ B of [0-9]+ B \([0-9]+ B headroom\)/high-water N B of N B (N B headroom)/g' \
   -e 's/^I \([0-9]+\)/I (N)/' \
   -e 's/\(elapsed=[0-9]+ms/(elapsed=Nms/g'
