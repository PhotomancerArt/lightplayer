#!/usr/bin/env bash
# PGO recipe for the ESP32-C6 emulator binary (D4: a `just` recipe, never a
# default build or CI step — the target is met without it).
#
#   just bench-emu-c6-pgo
#   scripts/emu/pgo-c6.sh
#
# Three phases, each in its own target dir so the RUSTFLAGS below never
# invalidate the plain `cargo build -p lp-emu-esp32c6 --release` other work
# in this tree depends on:
#
#   1. instrumented build (-Cprofile-generate) into target/emu-pgo/gen
#   2. one run of each pinned reference image (the "training" run) merged
#      into target/emu-pgo/merged.profdata with `cargo profdata -- merge`
#   3. optimized build (-Cprofile-use) into target/emu-pgo/use
#
# Needs `rustup component add llvm-tools-preview` and `cargo install
# cargo-binutils` (the `cargo profdata` subcommand); this script checks both
# and says so rather than failing deep in a build. The merged profile is a
# toolchain-bound artifact (the instrumentation counters are keyed to the
# exact compiler build) and is never committed — target/emu-pgo/ is
# gitignored the same way target/ is.
#
# Prints the optimized binary's path and runs the probe on it at the end
# (scripts/emu/bench-c6.sh --bin ... --no-build --no-promote), so the numbers
# land next to a same-window comparison without disturbing target/emu-bench/
# (the plain-build A/B's prev/ baseline).
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

if ! rustup component list --installed 2>/dev/null | grep -q '^llvm-tools-'; then
    echo "pgo-c6: llvm-tools-preview is not installed. Run:" >&2
    echo "    rustup component add llvm-tools-preview" >&2
    exit 1
fi
if ! command -v cargo-profdata >/dev/null 2>&1; then
    echo "pgo-c6: cargo-profdata is not installed. Run:" >&2
    echo "    cargo install cargo-binutils" >&2
    exit 1
fi

pgo_dir="target/emu-pgo"
raw_dir="$pgo_dir/raw"
merged="$repo/$pgo_dir/merged.profdata"

rm -rf "$raw_dir" "$merged"
mkdir -p "$raw_dir"

echo "pgo-c6: instrumented build (profile-generate)" >&2
RUSTFLAGS="-Cprofile-generate=$repo/$raw_dir" \
    cargo build -p lp-emu-esp32c6 --release --target-dir "$pgo_dir/gen"
gen_bin="$pgo_dir/gen/release/lp-emu-esp32c6"
[[ -x "$gen_bin" ]] || { echo "pgo-c6: $gen_bin is not an executable" >&2; exit 1; }

# The same two pinned reference images bench-c6.sh uses, run once each at t1
# (training wants coverage of the hot path, not a speed measurement, so one
# grade is enough) with UART0 discarded — this run's output does not feed
# any identity oracle.
reference_commit="d6cfaa205"
images=(
    "harness|LP_EMU_C6_REF_HARNESS|test_shader_compile_incremental,esp32c6,spike_uart0_link|5s|[inc-shader-compile] === DONE ==="
    "boot-idle-memfs|LP_EMU_C6_REF_BOOT_IDLE_MEMFS|esp32c6,server,radio,spike_uart0_link,memory_fs|3s|"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "pgo-c6: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$reference_commit-$slug/fw-esp32c6"
    if [[ ! -f "$path" ]]; then
        echo "pgo-c6: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh "$features" >&2
    fi
    echo "$path"
}

for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features")"
    echo "pgo-c6: training run: $slug" >&2
    if [[ -n "$exit_on" ]]; then
        "$gen_bin" --elf "$elf" --timeout "$timeout" --wall-timeout 120 \
            --exit-on "$exit_on" --uart0 stdout --time-grade t1 >/dev/null
    else
        "$gen_bin" --elf "$elf" --timeout "$timeout" --wall-timeout 120 \
            --uart0 stdout --time-grade t1 >/dev/null
    fi
done

echo "pgo-c6: merging profile data" >&2
cargo profdata -- merge -o "$merged" "$raw_dir"/*.profraw

echo "pgo-c6: optimized build (profile-use)" >&2
RUSTFLAGS="-Cprofile-use=$merged -Cllvm-args=-pgo-warn-missing-function" \
    cargo build -p lp-emu-esp32c6 --release --target-dir "$pgo_dir/use"
use_bin="$pgo_dir/use/release/lp-emu-esp32c6"
[[ -x "$use_bin" ]] || { echo "pgo-c6: $use_bin is not an executable" >&2; exit 1; }

echo "pgo-c6: PGO binary: $use_bin"
scripts/emu/bench-c6.sh --bin "$use_bin" --no-build --no-promote
