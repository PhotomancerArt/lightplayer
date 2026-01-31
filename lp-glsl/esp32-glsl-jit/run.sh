#!/bin/bash

set -e

cargo build --target riscv32imac-unknown-none-elf -p esp32-glsl-jit --release
probe-rs run --chip esp32c6 ../../../target/riscv32imac-unknown-none-elf/release/esp32-glsl-jit