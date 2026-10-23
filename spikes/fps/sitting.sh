#!/usr/bin/env bash
# SPIKE (fps spike): FS3 sittings. Hold the lease first
# (`board take fixture-s3 --as "s3-fps-spike" ...`).
#   sitting.sh known-good                       # the latest release, before and after
#   sitting.sh upload <project>                 # put a project on the board
#   sitting.sh run <image> <tag> [secs] [proj]  # flash an image (+upload), reboot-capture <secs>
set -uo pipefail
W=$(cd "$(dirname "$0")/../.." && pwd)
B=~/.photomancer/planning/lp2025/2026-10-20-2335-s3-radio-and-ota/fps-spike
OUT=$B/fs3; mkdir -p "$OUT"
MAC=D8:3B:DA:47:29:70
export BOARD_HOLDER="s3-fps-spike"
CLI=$W/target/release/lp-cli
port() { $CLI fwcheck port --mac $MAC 2>/dev/null | tail -1; }
PORT=${PORT:-$(port)}
echo "port: $PORT"
case "$1" in
  known-good)
    $CLI firmware install --release latest --mac $MAC --target esp32s3-8mb --yes 2>&1 | tail -3 ;;
  upload)
    timeout 120 $CLI upload "$2" "serial:$PORT" 2>&1 | tail -2 ;;
  run)
    img=$2; tag=$3; secs=${4:-45}; proj=${5:-}
    espflash flash --chip esp32s3 --partition-table "$W/lp-fw/fw-esp32s3/partitions.csv" \
      --flash-size 8mb --after hard-reset --port "$PORT" "$B/images/$img.elf" 2>&1 | tail -1
    sleep 4
    if [[ -n "$proj" ]]; then
      timeout 120 $CLI upload "$proj" "serial:$PORT" 2>&1 | tail -1
      sleep 2
    fi
    timeout $((secs+60)) $CLI link capture "$PORT" --console "$OUT/$img--$tag.txt" \
      --request reboot --seconds "$secs" > "$OUT/$img--$tag.cap.log" 2>&1; echo "capture rc=$?"
    grep -aE "\[perf\] frame=|\[stage\]|core1\]|pcs\] (probe|armed|round)|panic|PANIC|Guru|rror" \
      "$OUT/$img--$tag.txt" | grep -v '^M!' | cut -c1-200 | tail -12 ;;
  capture)
    tag=$2; secs=${3:-45}
    timeout $((secs+60)) $CLI link capture "$PORT" --console "$OUT/$tag.txt" \
      --request reboot --seconds "$secs" > "$OUT/$tag.cap.log" 2>&1; echo "capture rc=$?"
    grep -aE "\[perf\] frame=|\[stage\]|pcs\] (probe|armed|round)|panic|PANIC|Guru|rror" \
      "$OUT/$tag.txt" | grep -v '^M!' | cut -c1-200 | tail -12 ;;
esac
