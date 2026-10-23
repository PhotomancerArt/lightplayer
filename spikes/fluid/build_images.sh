#!/usr/bin/env bash
# SPIKE: the fluid spike's images, copied out of target/ (the hourly clean
# wipes it). build_images.sh s3|c6 <variant…>   ("bench" = the boot bench)
set -euo pipefail
W=$(cd "$(dirname "$0")/../.." && pwd); cd "$W"
F=~/.photomancer/planning/lp2025/2026-10-20-2335-s3-radio-and-ota/fluid-spike/images
chip=$1; shift
for v in "$@"; do
  if [[ $v == bench ]]; then export LP_FLUID_BENCH=1; unset LP_FLUID_VARIANT
  elif [[ $v == base ]]; then unset LP_FLUID_BENCH LP_FLUID_VARIANT
  else unset LP_FLUID_BENCH; export LP_FLUID_VARIANT=$v; fi
  if [[ $chip == s3 ]]; then
    just build-fw-esp32s3 >/tmp/fluid-build-$chip.log 2>&1 || { tail -30 /tmp/fluid-build-$chip.log; exit 1; }
    cp target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 "$F/s3-$v.elf"
  else
    just build-fw-esp32c6 >/tmp/fluid-build-$chip.log 2>&1 || { tail -30 /tmp/fluid-build-$chip.log; exit 1; }
    cp target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 "$F/c6-$v.elf"
  fi
  echo "built $chip $v"
done
