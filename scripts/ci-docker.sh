#!/bin/bash
# Run CI in Docker (x64) to reproduce GitHub runner environment.
# Requires: Docker running
# Usage: from lp2025 root: ./scripts/ci-docker.sh
#        or from photomancer root: ./lp2025/scripts/ci-docker.sh
#
# Uses --platform linux/amd64 to force x64 even on ARM Macs (QEMU emulation).

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LP2025_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PHOTOMANCER_ROOT="$(cd "$LP2025_ROOT/.." && pwd)"

if [ ! -d "$PHOTOMANCER_ROOT/lp-regalloc2" ]; then
  echo "Error: lp-regalloc2 not found at $PHOTOMANCER_ROOT/lp-regalloc2"
  echo "Clone it: cd $PHOTOMANCER_ROOT && git clone https://github.com/light-player/lp-regalloc2.git"
  exit 1
fi

echo "Building from $PHOTOMANCER_ROOT (lp2025 + lp-regalloc2)..."
cd "$PHOTOMANCER_ROOT"
docker build --platform linux/amd64 -f lp2025/Dockerfile.ci -t lp2025-ci .
docker run --platform linux/amd64 --rm lp2025-ci
