#!/bin/bash

set -e

# Get the workspace root (parent of lp-glsl directory)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$SCRIPT_DIR"

cargo build --target riscv32imac-unknown-none-elf --release --features esp32c6
probe-rs run --chip esp32c6 "$WORKSPACE_ROOT/target/riscv32imac-unknown-none-elf/release/esp32-glsl-jit"